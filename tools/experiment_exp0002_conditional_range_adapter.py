#!/usr/bin/env python3
"""Concrete conditional-range, cancellation, deadline, and coalescing model."""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from typing import Awaitable, Callable


class PreconditionFailed(IOError):
    pass


class ProtocolError(IOError):
    pass


class OperationCancelled(IOError):
    pass


class DeadlineExceeded(IOError):
    pass


@dataclass(frozen=True)
class RangeResponse:
    status: int
    etag: str
    start: int
    end_inclusive: int
    total: int
    body: bytes


@dataclass
class RequestBudget:
    max_requests: int
    used: int = 0

    def charge(self) -> None:
        if self.used >= self.max_requests:
            raise DeadlineExceeded("logical request deadline exceeded")
        self.used += 1


class IfMatchStore:
    def __init__(self, content: bytes, etag: str) -> None:
        require_strong_token(etag)
        self.content = content
        self.etag = etag
        self.requests = 0
        self.bytes_sent = 0
        self.next_fault: str | None = None

    def publish(self, content: bytes, etag: str) -> None:
        require_strong_token(etag)
        self.content = content
        self.etag = etag

    def conditional_range(self, expected: str, offset: int, length: int) -> RangeResponse:
        require_strong_token(expected)
        self.requests += 1
        if expected != self.etag:
            raise PreconditionFailed("If-Match failed")
        if offset < 0 or length < 0 or offset + length > len(self.content):
            raise EOFError("range outside object")

        body = self.content[offset : offset + length]
        response = RangeResponse(
            206,
            self.etag,
            offset,
            offset + length - 1,
            len(self.content),
            body,
        )
        fault = self.next_fault
        self.next_fault = None
        if fault == "weak-etag":
            response = RangeResponse(
                response.status,
                f"W/{response.etag}",
                response.start,
                response.end_inclusive,
                response.total,
                response.body,
            )
        elif fault == "wrong-etag":
            response = RangeResponse(
                response.status,
                '"other"',
                response.start,
                response.end_inclusive,
                response.total,
                response.body,
            )
        elif fault == "wrong-range":
            response = RangeResponse(
                response.status,
                response.etag,
                response.start + 1,
                response.end_inclusive + 1,
                response.total,
                response.body,
            )
        elif fault == "short-body":
            response = RangeResponse(
                response.status,
                response.etag,
                response.start,
                response.end_inclusive,
                response.total,
                response.body[:-1],
            )
        self.bytes_sent += len(response.body)
        return response


class ConditionalAdapter:
    def __init__(self, store: IfMatchStore) -> None:
        self.store = store
        self.accepted_bytes = 0

    def read_exact(
        self,
        expected: str,
        offset: int,
        length: int,
        budget: RequestBudget,
        cancelled: Callable[[], bool],
    ) -> bytes:
        require_strong_token(expected)
        if cancelled():
            raise OperationCancelled("operation cancelled")
        budget.charge()
        response = self.store.conditional_range(expected, offset, length)
        if response.status != 206:
            raise ProtocolError("range status")
        require_strong_token(response.etag)
        if response.etag != expected:
            raise ProtocolError("response version token")
        if (
            response.start != offset
            or response.end_inclusive != offset + length - 1
            or response.total != len(self.store.content)
        ):
            raise ProtocolError("content range")
        if len(response.body) != length:
            raise ProtocolError("short response")
        self.accepted_bytes += length
        return response.body


def require_strong_token(token: str) -> None:
    if not token or token.startswith("W/") or not (token.startswith('"') and token.endswith('"')):
        raise ProtocolError("strong version token required")


def two_range_operation(
    adapter: ConditionalAdapter,
    expected: str,
    budget: RequestBudget,
    after_first: Callable[[], None] | None = None,
    cancelled: Callable[[], bool] = lambda: False,
) -> bytes:
    first = adapter.read_exact(expected, 0, 4, budget, cancelled)
    if after_first is not None:
        after_first()
    second = adapter.read_exact(expected, 4, 4, budget, cancelled)
    return first + second


class AsyncImmutableVersionStore:
    def __init__(self, versions: dict[str, bytes], gate: asyncio.Event) -> None:
        for token in versions:
            require_strong_token(token)
        self.versions = versions
        self.gate = gate
        self.requests = 0

    async def read_exact(self, token: str, offset: int, length: int) -> bytes:
        require_strong_token(token)
        self.requests += 1
        await self.gate.wait()
        content = self.versions[token]
        body = content[offset : offset + length]
        if len(body) != length:
            raise EOFError("short immutable-version range")
        return body


class VersionKeyedCoalescer:
    def __init__(self, source: AsyncImmutableVersionStore) -> None:
        self.source = source
        self.lock = asyncio.Lock()
        self.inflight: dict[tuple[str, int, int], asyncio.Task[bytes]] = {}

    async def read_exact(self, token: str, offset: int, length: int) -> bytes:
        key = (token, offset, length)
        async with self.lock:
            task = self.inflight.get(key)
            if task is None:
                task = asyncio.create_task(self.source.read_exact(token, offset, length))
                self.inflight[key] = task
        try:
            return await asyncio.shield(task)
        finally:
            if task.done():
                async with self.lock:
                    if self.inflight.get(key) is task:
                        del self.inflight[key]


async def exercise_coalescing() -> None:
    token_a = '"version-a"'
    token_b = '"version-b"'
    gate = asyncio.Event()
    source = AsyncImmutableVersionStore(
        {token_a: b"abcdefgh", token_b: b"ABCDEFGH"},
        gate,
    )
    coalescer = VersionKeyedCoalescer(source)

    cancelled_waiter = asyncio.create_task(coalescer.read_exact(token_a, 0, 4))
    surviving_waiter = asyncio.create_task(coalescer.read_exact(token_a, 0, 4))
    await asyncio.sleep(0)
    await asyncio.sleep(0)
    assert source.requests == 1
    cancelled_waiter.cancel()
    gate.set()
    try:
        await cancelled_waiter
    except asyncio.CancelledError:
        pass
    else:
        raise AssertionError("cancelled waiter unexpectedly completed")
    assert await surviving_waiter == b"abcd"
    assert source.requests == 1

    gate.clear()
    first_version = asyncio.create_task(coalescer.read_exact(token_a, 4, 4))
    second_version = asyncio.create_task(coalescer.read_exact(token_b, 4, 4))
    await asyncio.sleep(0)
    await asyncio.sleep(0)
    assert source.requests == 3
    gate.set()
    assert await first_version == b"efgh"
    assert await second_version == b"EFGH"
    assert not coalescer.inflight


def main() -> None:
    token_a = '"version-a"'
    token_b = '"version-b"'
    content_a = b"abcdefgh"
    content_b = b"ABCDEFGH"

    try:
        require_strong_token('W/"weak"')
    except ProtocolError:
        pass
    else:
        raise AssertionError("weak token was accepted")

    changed_store = IfMatchStore(content_a, token_a)
    changed_adapter = ConditionalAdapter(changed_store)
    try:
        two_range_operation(
            changed_adapter,
            token_a,
            RequestBudget(4),
            after_first=lambda: changed_store.publish(content_b, token_b),
        )
    except PreconditionFailed:
        pass
    else:
        raise AssertionError("version change produced a mixed result")
    assert changed_adapter.accepted_bytes == 4

    restarted_adapter = ConditionalAdapter(changed_store)
    assert two_range_operation(restarted_adapter, token_b, RequestBudget(4)) == content_b
    assert restarted_adapter.accepted_bytes == 8

    cancelled_flag = False

    def cancel_after_first() -> None:
        nonlocal cancelled_flag
        cancelled_flag = True

    cancelled_adapter = ConditionalAdapter(IfMatchStore(content_a, token_a))
    try:
        two_range_operation(
            cancelled_adapter,
            token_a,
            RequestBudget(4),
            after_first=cancel_after_first,
            cancelled=lambda: cancelled_flag,
        )
    except OperationCancelled:
        pass
    else:
        raise AssertionError("cancelled operation produced a result")
    assert cancelled_adapter.accepted_bytes == 4

    deadline_adapter = ConditionalAdapter(IfMatchStore(content_a, token_a))
    try:
        two_range_operation(deadline_adapter, token_a, RequestBudget(1))
    except DeadlineExceeded:
        pass
    else:
        raise AssertionError("expired deadline produced a result")
    assert deadline_adapter.accepted_bytes == 4

    for fault in ("weak-etag", "wrong-etag", "wrong-range", "short-body"):
        store = IfMatchStore(content_a, token_a)
        store.next_fault = fault
        adapter = ConditionalAdapter(store)
        try:
            adapter.read_exact(token_a, 0, 4, RequestBudget(1), lambda: False)
        except ProtocolError:
            pass
        else:
            raise AssertionError(f"malformed {fault} response was accepted")
        assert adapter.accepted_bytes == 0

    asyncio.run(exercise_coalescing())

    print("strong_token_requirement=pass")
    print("if_match_version_change_is_terminal=pass")
    print("whole_operation_restart_uses_new_token=pass")
    print("cancellation_is_terminal=pass")
    print("deadline_is_terminal=pass")
    print("response_metadata_validation=pass")
    print("same_version_request_coalescing=pass")
    print("waiter_cancellation_does_not_cancel_shared_read=pass")
    print("different_versions_never_coalesce=pass")
    print("finding=accepted bytes from a failed operation are never reusable assurance state")
    print("finding=coalescing keys must include the strong immutable version token")


if __name__ == "__main__":
    main()
