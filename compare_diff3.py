#!/usr/bin/env python3

import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


@dataclass
class FileVersions:
    myfile: Any
    oldfile: Any
    yourfile: Any

    @classmethod
    def from_iterable(cls, iterable: Iterable) -> "FileVersions":
        return cls(*iterable)

    def __iter__(self):
        yield self.myfile
        yield self.oldfile
        yield self.yourfile


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

    refs = (parent1, merge_base, parent2)  # myfile, oldfile, yourfile
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
        myfile=output_dir / 'myfile.txt',
        oldfile=output_dir / 'oldfile.txt',
        yourfile=output_dir / 'yourfile.txt',
    )
    for path, content in zip(paths, file_versions):
        path.write_text(content)
    return paths


def _run_diff3_command_executor(paths: FileVersions, is_local: bool, diff3_options: list[str]) -> str:
    cmd = []
    if is_local:
        cmd = ['cargo', 'run', '--', 'diff3'] + diff3_options
    else:
        cmd = ['diff3'] + diff3_options

    full_cmd = cmd + [str(paths.myfile.absolute()), str(paths.oldfile.absolute()), str(paths.yourfile.absolute())]

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


def _run_diff3_commands(paths: FileVersions, diff3_options: list[str]) -> tuple[str, str]:
    """Runs the local and system diff3 commands."""
    local_diff3 = _run_diff3_command_executor(paths, is_local=True, diff3_options=diff3_options)
    system_diff3 = _run_diff3_command_executor(paths, is_local=False, diff3_options=diff3_options)

    return local_diff3, system_diff3


def run_diff3(file_versions: FileVersions, output_dir: Path, diff3_options: list[str]) -> tuple[FileVersions, str, str]:
    """Runs diff3 on the three file versions and returns paths and results."""
    paths = _write_version_files(output_dir, file_versions)
    local_diff3, system_diff3 = _run_diff3_commands(paths, diff3_options)
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

    if (
        file_versions.myfile == file_versions.oldfile and
        file_versions.yourfile == file_versions.oldfile
    ):
        print(f'      Skipping {filename}: All three versions are ' 'identical.',
              file=sys.stderr)
        return None

    if file_versions.myfile == file_versions.yourfile:
        print(f'      Skipping {filename}: Myfile and Yourfile versions ' 'are identical.',
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


def apply_ed_script(file_path: Path, script: str):
    """Applies an ed script to a file."""
    # ed doesn't create a file, so we need to handle that.
    # The parent directory is guaranteed to exist.
    if not file_path.exists():
        file_path.touch()
    # The ed script needs a trailing newline
    if not script.endswith('\n'):
        script += '\n'
    # The ed script needs a `w` command to write the file and a `q` command to quit
    script += 'w\nq\n'

    try:
        subprocess.run(
            ['ed', str(file_path)],
            input=script,
            text=True,
            capture_output=True,
            check=True,
        )
    except subprocess.CalledProcessError as e:
        stderr = e.stderr.strip()
        if stderr == '?':
            # ed outputs '?' to stderr for many errors.  Unfortunately, we
            # can't get any more information than that.
            print(f"Error applying ed script to {file_path}", file=sys.stderr)
        else:
            print(f"Error applying ed script to {file_path}: {stderr}", file=sys.stderr)


def _are_outputs_equivalent(local_diff3: str, system_diff3: str, diff3_options: list[str], paths: FileVersions) -> bool:
    """Compares the outputs of local and system diff3 commands."""
    if local_diff3 == system_diff3:
        return True

    ed_output_options = ['-e', '--ed', '-A', '--show-all', '-E', '--show-overlap', '-3', '--easy-only', '-x', '--overlap-only', '-X']
    is_ed_output_option_present = any(opt in diff3_options for opt in ed_output_options)
    is_merge_present = any(opt in diff3_options for opt in ['-m', '--merge'])

    is_ed_output = is_ed_output_option_present and not is_merge_present
    if not is_ed_output:
        return False

    with tempfile.TemporaryDirectory() as temp_dir:
        temp_dir_path = Path(temp_dir)
        local_result_path = temp_dir_path / 'local_result.txt'
        system_result_path = temp_dir_path / 'system_result.txt'

        shutil.copy(paths.myfile, local_result_path)
        shutil.copy(paths.myfile, system_result_path)

        apply_ed_script(local_result_path, local_diff3)
        apply_ed_script(system_result_path, system_diff3)

        return local_result_path.read_text() == system_result_path.read_text()


def main():
    """Main function."""
    diff3_options = sys.argv[1:]
    comparison_count = 0
    for commit, filename, file_versions in _iter_conflicted_files(GitStrategy):
        print(
            f'      Performing diff3 comparison for {filename}...', file=sys.stderr)
        output_dir = (Path('diff_outputs') /
                      f"{filename.replace('/', '_')}_{commit}")
        paths, local_diff3, system_diff3 = run_diff3(
            file_versions, output_dir, diff3_options)

        if _are_outputs_equivalent(local_diff3, system_diff3, diff3_options, paths):
            if output_dir.exists():
                shutil.rmtree(output_dir)
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
