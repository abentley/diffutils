#!/usr/bin/env python3

import subprocess
import os
import sys
from pathlib import Path
from dataclasses import dataclass
from typing import Any, Iterable


@dataclass
class FileVersions:
    ours: Any
    base: Any
    theirs: Any

    @classmethod
    def from_iterable(cls, iterable: Iterable) -> "FileVersions":
        return cls(*iterable)

    def __iter__(self):
        yield self.ours
        yield self.base
        yield self.theirs


def _run_command(cmd, check_result=True):
    """Helper function to run a subprocess command."""
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=check_result,
        )
        return result.stdout.strip()
    except subprocess.CalledProcessError as e:
        print(f"Error running command: {' '.join(cmd)}.\nError: {e}")
        return ""


def get_merge_commits():
    """Gets a list of merge commits from the git history."""
    result = _run_command(["git", "log", "--merges", "--pretty=%H"])
    return result.splitlines()


def _get_changed_files(ref1, ref2):
    """Get files changed between two refs."""
    diff_result = _run_command(["git", "diff", "--name-only", ref1, ref2])
    return set(diff_result.splitlines())


def get_conflicted_files(commit):
    """Gets a list of files that were changed in a merge commit and
    potentially had conflicts."""
    parent1 = f"{commit}^1"
    parent2 = f"{commit}^2"
    merge_base = _run_command(["git", "merge-base", parent1, parent2])

    files_in_parent1 = _get_changed_files(merge_base, parent1)
    files_in_parent2 = _get_changed_files(merge_base, parent2)

    # Files that were changed in both branches relative to the merge base
    # are candidates for conflicts
    conflicted_candidates = files_in_parent1.intersection(files_in_parent2)
    return [f for f in conflicted_candidates if f != ""]


def _get_file_version(ref, filename):
    """Gets a specific version of a file."""
    return _run_command(["git", "show", f"{ref}:{filename}"],
                        check_result=False)


def get_file_versions(commit, filename) -> FileVersions | None:
    """Gets the three versions of a file from a merge commit."""
    parent1 = f"{commit}^1"
    parent2 = f"{commit}^2"
    merge_base = _run_command(["git", "merge-base", parent1, parent2])

    refs = (parent1, merge_base, parent2)  # ours, base, theirs
    versions_iterable = (_get_file_version(ref, filename) for ref in refs)
    versions = FileVersions.from_iterable(versions_iterable)

    if not any(versions):
        return None
    return versions


def _write_version_files(output_dir: Path, file_versions: FileVersions) -> FileVersions:
    """Writes the version files to the output directory and returns their paths."""
    output_dir.mkdir(parents=True, exist_ok=True)
    paths = FileVersions(
        ours=output_dir / "ours.txt",
        base=output_dir / "base.txt",
        theirs=output_dir / "theirs.txt",
    )
    for path, content in zip(paths, file_versions):
        path.write_text(content)
    return paths


def _run_diff3_commands(paths: FileVersions) -> tuple[str, str]:
    """Runs the local and system diff3 commands."""

    def run_diff3_command(cmd):
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
        )
        if result.returncode not in [0, 1]:
            print(f"Error running command: {' '.join(cmd)}.\nError: {result.stderr}")
            return ""
        return result.stdout.strip()

    local_cmd = ['cargo', "run", "--", "diff3", *[str(p) for p in paths]]
    local_diff3 = run_diff3_command(local_cmd)

    system_cmd = ["diff3", "-m", *[str(p) for p in paths]]
    system_diff3 = run_diff3_command(system_cmd)

    return local_diff3, system_diff3


def run_diff3(file_versions: FileVersions, output_dir: Path) -> tuple[FileVersions, str, str]:
    """Runs diff3 on the three file versions and returns paths and results."""
    paths = _write_version_files(output_dir, file_versions)
    local_diff3, system_diff3 = _run_diff3_commands(paths)
    return paths, local_diff3, system_diff3


def _iter_conflicted_files():
    """A generator that yields commits and their truly conflicted files."""
    merge_commits = get_merge_commits()
    print(f"Found {len(merge_commits)} merge commits.")
    for commit in merge_commits:
        conflicted_files = get_conflicted_files(commit)
        print(
            f"  Found {len(conflicted_files)} changed files in commit {commit}."
        )
        for filename in conflicted_files:
            if filename == "":
                continue
            print(f"    Checking file: {filename} in commit {commit}...")
            file_versions = get_file_versions(commit, filename)

            if file_versions is None:
                print(
                    f"      Skipping {filename}: Could not get all three "
                    "file versions."
                )
                continue

            if (file_versions.ours == file_versions.base and
                    file_versions.theirs == file_versions.base):
                print(f"      Skipping {filename}: All three versions are "
                      "identical.")
                continue

            if file_versions.ours == file_versions.theirs:
                print(f"      Skipping {filename}: Ours and Theirs versions "
                      "are identical.")
                continue
            yield commit, filename, file_versions


def main():
    """Main function."""
    comparison_count = 0
    for commit, filename, file_versions in _iter_conflicted_files():
        print(f"      Performing diff3 comparison for {filename}...")
        output_dir = (Path("diff_outputs") /
                    f"{filename.replace('/', '_')}_{commit}")
        paths, local_diff3, system_diff3 = run_diff3(file_versions, output_dir)

        if local_diff3 == system_diff3:
            for path in paths:
                path.unlink()
            output_dir.rmdir()
        else:
            (output_dir / "local_diff3.txt").write_text(local_diff3)
            (output_dir / "system_diff3.txt").write_text(system_diff3)
            print(f"  Outputs differ for {filename} in {commit}")
            print(
                "  Saved file versions and diff outputs to "
                f"{output_dir}"
            )
            comparison_count += 1

    print(f"Total comparisons made: {comparison_count}")


if __name__ == "__main__":
    main()