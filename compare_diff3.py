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


class GitStrategy:
    @classmethod
    def _run_git_command(cls, cmd_args, check_result=True):
        """Helper function to run a git subprocess command."""
        full_cmd = ["git"] + cmd_args
        try:
            result = subprocess.run(
                full_cmd,
                capture_output=True,
                text=True,
                check=check_result,
            )
            return result.stdout.strip()
        except subprocess.CalledProcessError as e:
            print(
                f"Error running command: {' '.join(full_cmd)}.\nError: {e}", file=sys.stderr)
            return ""

    @classmethod
    def get_merge_commits(cls):
        """Gets a list of merge commits from the git history."""
        result = cls._run_git_command(["log", "--merges", "--pretty=%H"])
        return result.splitlines()

    @classmethod
    def _get_changed_files(cls, ref1, ref2):
        """Get files changed between two refs."""
        diff_result = cls._run_git_command(["diff", "--name-only", ref1, ref2])
        return set(diff_result.splitlines())

    @classmethod
    def get_merge_base(cls, commit):
        parent1 = f"{commit}^1"
        parent2 = f"{commit}^2"
        return cls._run_git_command(["merge-base", parent1, parent2])

    @classmethod
    def _get_file_version(cls, ref, filename):
        """Gets a specific version of a file."""
        return cls._run_git_command(["show", f"{ref}:{filename}"],
                                    check_result=False)


def get_conflicted_files(strategy, commit):
    """Gets a list of files that were changed in a merge commit and
    potentially had conflicts."""
    merge_base = strategy.get_merge_base(commit)
    parent1 = f"{commit}^1"
    parent2 = f"{commit}^2"

    files_in_parent1 = strategy._get_changed_files(merge_base, parent1)
    files_in_parent2 = strategy._get_changed_files(merge_base, parent2)

    # Files that were changed in both branches relative to the merge base
    # are candidates for conflicts
    conflicted_candidates = files_in_parent1.intersection(files_in_parent2)
    return [f for f in conflicted_candidates if f != ""]


def get_file_versions(strategy, commit, filename) -> FileVersions | None:
    """Gets the three versions of a file from a merge commit."""
    merge_base = strategy.get_merge_base(commit)
    parent1 = f"{commit}^1"
    parent2 = f"{commit}^2"

    refs = (parent1, merge_base, parent2)  # ours, base, theirs
    versions_iterable = (strategy._get_file_version(ref, filename)
                         for ref in refs)
    versions = FileVersions.from_iterable(versions_iterable)

    if not any(versions):
        return None
    return versions


def _write_version_files(output_dir: Path, file_versions: FileVersions) -> FileVersions:
    """Writes the version files to the output directory and returns their paths."""
    output_dir.mkdir(parents=True, exist_ok=True)
    paths = FileVersions(
        ours=output_dir / 'ours.txt',
        base=output_dir / 'base.txt',
        theirs=output_dir / 'theirs.txt',
    )
    for path, content in zip(paths, file_versions):
        path.write_text(content)
    return paths


def _run_diff3_command_executor(paths: FileVersions, is_local: bool) -> str:
    cmd = []
    if is_local:
        cmd = ['cargo', 'run', '--', 'diff3']
    else:
        cmd = ['diff3', '-m']

    full_cmd = cmd + [str(paths.ours.absolute()), str(paths.base.absolute()), str(paths.theirs.absolute())]

    print(f"Running command: {' '.join(full_cmd)}", file=sys.stderr)
    result = subprocess.run(
        full_cmd,
        capture_output=True,
        text=True,
        cwd="/home/abentley/hacking/diffutils",
    )
    if result.returncode not in [0, 1]:
        print(
            f"Error running command: {' '.join(full_cmd)}.\nError: {result.stderr}", file=sys.stderr)
        return ""
    return result.stdout.strip()


def _run_diff3_commands(paths: FileVersions, executor) -> tuple[str, str]:
    """Runs the local and system diff3 commands."""
    local_diff3 = executor(paths, is_local=True)
    system_diff3 = executor(paths, is_local=False)

    return local_diff3, system_diff3


def run_diff3(file_versions: FileVersions, output_dir: Path) -> tuple[FileVersions, str, str]:
    """Runs diff3 on the three file versions and returns paths and results."""
    paths = _write_version_files(output_dir, file_versions)
    local_diff3, system_diff3 = _run_diff3_commands(
        paths, executor=_run_diff3_command_executor)
    return paths, local_diff3, system_diff3


def _get_and_filter_file_versions(strategy, commit, filename):
    """Gets file versions and returns them if they are truly conflicted, else None."""
    print(
        f'    Checking file: {filename} in commit {commit}...', file=sys.stderr)
    file_versions = get_file_versions(strategy, commit, filename)

    if file_versions is None:
        print(
            f'      Skipping {filename}: Could not get all three '
            'file versions.',
            file=sys.stderr
        )
        return None

    if (file_versions.ours == file_versions.base and
            file_versions.theirs == file_versions.base):
        print(f'      Skipping {filename}: All three versions are ' 'identical.',
              file=sys.stderr)
        return None

    if file_versions.ours == file_versions.theirs:
        print(f'      Skipping {filename}: Ours and Theirs versions ' 'are identical.',
              file=sys.stderr)
        return None
    return file_versions


def _iter_conflicted_files(strategy):
    """A generator that yields commits and their truly conflicted files."""
    merge_commits = strategy.get_merge_commits()
    print(f"Found {len(merge_commits)} merge commits.", file=sys.stderr)
    for commit in merge_commits:
        conflicted_files = get_conflicted_files(strategy, commit)
        print(
            f"  Found {len(conflicted_files)} changed files in commit {commit}.",
            file=sys.stderr
        )
        for filename in conflicted_files:
            if filename == '':
                continue

            file_versions = _get_and_filter_file_versions(
                strategy, commit, filename)
            if file_versions is not None:
                yield commit, filename, file_versions


def main():
    """Main function."""
    comparison_count = 0
    for commit, filename, file_versions in _iter_conflicted_files(GitStrategy):
        print(
            f'      Performing diff3 comparison for {filename}...', file=sys.stderr)
        output_dir = (Path('diff_outputs') /
                      f"{filename.replace('/', '_')}_{commit}")
        paths, local_diff3, system_diff3 = run_diff3(file_versions, output_dir)

        if local_diff3 == system_diff3:
            for path in paths:
                path.unlink()
            output_dir.rmdir()
        else:
            (output_dir / 'local_diff3.txt').write_text(local_diff3)
            (output_dir / 'system_diff3.txt').write_text(system_diff3)
            print(
                f"  Outputs differ for {filename} in {commit}", file=sys.stderr)
            print(
                '  Saved file versions and diff outputs to '
                f'{output_dir}',
                file=sys.stderr
            )
            comparison_count += 1

    print(f"Total comparisons made: {comparison_count}", file=sys.stderr)


if __name__ == "__main__":
    main()
