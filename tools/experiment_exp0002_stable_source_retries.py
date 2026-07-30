#!/usr/bin/env python3
"""Model retry, cancellation, and restart rules for stable range sources."""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum, auto


class SourceChanged(IOError):
    pass


class Cancelled(IOError):
    pass


class Transient(IOError):
    pass


class EventKind(Enum):
    DATA = auto()
    TRANSIENT_BEFORE_BYTES = auto()
    TRANSIENT_AFTER_PARTIAL = auto()
    VERSION_CHANGE = auto()
    CANCEL = auto()


@dataclass(frozen=True)
class Event:
    kind: EventKind
    data: bytes = b""
    version: bytes | None = None


@dataclass
class ScriptedConditionalSource:
    content: bytes
    version: bytes
    events: list[Event]
    requests: int = 0
    bytes_received: int = 0
    discarded_partial_bytes: int = 0

    def conditional_read(self, expected: bytes, offset: int, length: int) -> bytes:
        self.requests += 1
        if self.version != expected:
            raise SourceChanged("precondition version mismatch")
        event = self.events.pop(0) if self.events else Event(EventKind.DATA)
        if event.kind is EventKind.VERSION_CHANGE:
            if event.version is None:
                raise AssertionError("version change requires a version")
            self.version = event.version
            raise SourceChanged("version changed during request")
        if event.kind is EventKind.TRANSIENT_BEFORE_BYTES:
            raise Transient("transient failure before response bytes")
        if event.kind is EventKind.TRANSIENT_AFTER_PARTIAL:
            partial = event.data or self.content[offset : offset + max(1, length // 2)]
            self.bytes_received += len(partial)
            self.discarded_partial_bytes += len(partial)
            raise Transient("transient failure after partial response")
        if event.kind is EventKind.CANCEL:
            raise Cancelled("operation cancelled")
        data = self.content[offset : offset + length]
        if len(data) != length:
            raise EOFError("short conditional range")
        self.bytes_received += len(data)
        return data


def read_exact_with_same_version_retries(
    source: ScriptedConditionalSource,
    expected: bytes,
    offset: int,
    length: int,
    max_attempts: int,
) -> bytes:
    if max_attempts <= 0:
        raise ValueError("max_attempts must be positive")
    for attempt in range(1, max_attempts + 1):
        try:
            data = source.conditional_read(expected, offset, length)
        except Transient:
            if attempt == max_attempts:
                raise
            continue
        # No bytes become visible until one complete same-version response exists.
        return data
    raise AssertionError("unreachable")


def two_range_operation(
    source: ScriptedConditionalSource,
    expected: bytes,
    max_attempts: int = 3,
) -> bytes:
    first = read_exact_with_same_version_retries(source, expected, 0, 4, max_attempts)
    second = read_exact_with_same_version_retries(source, expected, 4, 4, max_attempts)
    return first + second


def main() -> None:
    version_a = b"A" * 32
    version_b = b"B" * 32
    content_a = b"abcdefgh"
    content_b = b"ABCDEFGH"

    before = ScriptedConditionalSource(
        content_a,
        version_a,
        [Event(EventKind.TRANSIENT_BEFORE_BYTES), Event(EventKind.DATA), Event(EventKind.DATA)],
    )
    assert two_range_operation(before, version_a) == content_a
    assert before.requests == 3
    assert before.discarded_partial_bytes == 0

    partial = ScriptedConditionalSource(
        content_a,
        version_a,
        [Event(EventKind.TRANSIENT_AFTER_PARTIAL, b"ab"), Event(EventKind.DATA), Event(EventKind.DATA)],
    )
    assert two_range_operation(partial, version_a) == content_a
    assert partial.discarded_partial_bytes == 2
    assert partial.bytes_received == 10  # two discarded bytes plus eight accepted bytes

    changed = ScriptedConditionalSource(
        content_a,
        version_a,
        [Event(EventKind.DATA), Event(EventKind.VERSION_CHANGE, version=version_b)],
    )
    try:
        two_range_operation(changed, version_a)
    except SourceChanged:
        pass
    else:
        raise AssertionError("version change was retried into a mixed-version result")
    assert changed.requests == 2

    cancelled = ScriptedConditionalSource(
        content_a,
        version_a,
        [Event(EventKind.DATA), Event(EventKind.CANCEL)],
    )
    try:
        two_range_operation(cancelled, version_a)
    except Cancelled:
        pass
    else:
        raise AssertionError("cancelled operation produced a result")

    # A caller may start a completely new operation with a new token. The new
    # result is independent; bytes from the failed A operation are not reused.
    restarted = ScriptedConditionalSource(content_b, version_b, [])
    assert two_range_operation(restarted, version_b) == content_b

    exhausted = ScriptedConditionalSource(
        content_a,
        version_a,
        [Event(EventKind.TRANSIENT_BEFORE_BYTES)] * 3,
    )
    try:
        two_range_operation(exhausted, version_a, max_attempts=3)
    except Transient:
        pass
    else:
        raise AssertionError("retry exhaustion produced a result")
    assert exhausted.requests == 3

    print("same_version_retry_before_bytes=pass")
    print("partial_response_discard_and_retry=pass")
    print("version_change_is_terminal=pass")
    print("cancellation_is_terminal=pass")
    print("whole_operation_restart_with_new_token=pass")
    print("retry_attempt_limit=pass")
    print(f"partial_case_wire_bytes={partial.bytes_received}")
    print(f"partial_case_discarded_bytes={partial.discarded_partial_bytes}")
    print("finding=individual retries are safe only against the same expected immutable version")
    print("finding=partial response bytes must be discarded before retry")
    print("finding=version mismatch, cancellation, and deadline are terminal for one assurance operation")
    print("finding=a new version requires a new operation with no reused partial result")


if __name__ == "__main__":
    main()
