#!/usr/bin/env python3
"""Self-tests for tools/plan_phase3_private_inodes.py."""

from __future__ import annotations

import unittest

from tools import plan_phase3_private_inodes as planner


class PrivateInodePlannerTests(unittest.TestCase):
    def test_single_initial_run_has_no_merge_output_overlap(self) -> None:
        plan = planner.normal_inode_plan(1)
        self.assertEqual(plan.spill_run_peak, 1)
        self.assertEqual(plan.sort_window, 3)
        self.assertEqual(plan.output_tree_window, 7)
        self.assertEqual(plan.required_additional_inodes, 7)

    def test_two_initial_runs_create_one_merge_output_overlap(self) -> None:
        plan = planner.normal_inode_plan(2)
        self.assertEqual(plan.spill_run_peak, 3)
        self.assertEqual(plan.sort_window, 5)
        self.assertEqual(plan.required_additional_inodes, 7)

    def test_sort_window_overtakes_fixed_output_window(self) -> None:
        plan = planner.normal_inode_plan(5)
        self.assertEqual(plan.spill_run_peak, 6)
        self.assertEqual(plan.sort_window, 8)
        self.assertEqual(plan.required_additional_inodes, 8)

    def test_large_initial_run_limit_scales_linearly_without_fanout_guessing(self) -> None:
        plan = planner.normal_inode_plan(64)
        self.assertEqual(plan.spill_run_peak, 65)
        self.assertEqual(plan.sort_window, 67)
        self.assertEqual(plan.required_additional_inodes, 67)

    def test_crash_resume_includes_checkpoint_before_reclamation(self) -> None:
        plan = planner.crash_resume_inode_plan()
        self.assertEqual(plan.output_tree_window, 4)
        self.assertEqual(plan.terminal_compaction_window, 5)
        self.assertEqual(plan.required_additional_inodes, 5)

    def test_unified_plan_uses_larger_normal_or_restart_requirement(self) -> None:
        small = planner.unified_inode_plan(1)
        self.assertEqual(small.required_additional_inodes, 7)
        large = planner.unified_inode_plan(10)
        self.assertEqual(large.required_additional_inodes, 13)

    def test_zero_initial_run_limit_fails(self) -> None:
        with self.assertRaisesRegex(planner.InodePlanError, "must be positive"):
            planner.unified_inode_plan(0)

    def test_hard_link_publication_is_not_counted_as_another_inode(self) -> None:
        plan = planner.normal_inode_plan(1)
        # The fixed output-tree window has exactly one staged canonical output
        # inode even though publication adds a destination hard-link name.
        self.assertEqual(plan.output_tree_window, 7)


if __name__ == "__main__":
    unittest.main()
