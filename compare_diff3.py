#!/usr/bin/env python3

import subprocess
import os
import sys

def get_merge_commits():
    """Gets a list of merge commits from the git history."""
    result = subprocess.run(
        ["git", "log", "--merges", "--pretty=%H"],
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout.strip().split("\n")

def get_conflicted_files(commit):
    """Gets a list of files that were changed in a merge commit and potentially had conflicts."""
    try:
        parent1 = f"{commit}^1"
        parent2 = f"{commit}^2"
        merge_base = subprocess.run(
            ["git", "merge-base", parent1, parent2],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()

        # Get files changed between merge_base and parent1
        diff1_result = subprocess.run(
            ["git", "diff", "--name-only", merge_base, parent1],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        files_in_parent1 = set(diff1_result.splitlines())

        # Get files changed between merge_base and parent2
        diff2_result = subprocess.run(
            ["git", "diff", "--name-only", merge_base, parent2],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        files_in_parent2 = set(diff2_result.splitlines())

        # Files that were changed in both branches relative to the merge base are candidates for conflicts
        conflicted_candidates = list(files_in_parent1.intersection(files_in_parent2))
        return [f for f in conflicted_candidates if f]
    except subprocess.CalledProcessError:
        return []

def get_file_versions(commit, filename):
    """Gets the three versions of a file from a merge commit."""
    try:
        merge_base = subprocess.run(
            ["git", "merge-base", f"{commit}^1", f"{commit}^2"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()

        base = subprocess.run(
            ["git", "show", f"{merge_base}:{filename}"],
            capture_output=True,
            text=True,
        ).stdout
        ours = subprocess.run(
            ["git", "show", f"{commit}^1:{filename}"],
            capture_output=True,
            text=True,
        ).stdout
        theirs = subprocess.run(
            ["git", "show", f"{commit}^2:{filename}"],
            capture_output=True,
            text=True,
        ).stdout
        if not base or not ours or not theirs:
            return None
        return base, ours, theirs
    except subprocess.CalledProcessError:
        return None

def run_diff3(file_versions):
    """Runs diff3 on the three file versions."""
    base, ours, theirs = file_versions
    with open("base.txt", "w") as f:
        f.write(base)
    with open("ours.txt", "w") as f:
        f.write(ours)
    with open("theirs.txt", "w") as f:
        f.write(theirs)

    cargo_path = "/home/abentley/.cargo/bin/cargo" # TODO: Replace with the actual path to cargo

    # Run the local diff3 implementation with merge option
    local_diff3 = subprocess.run(
        ['cargo', "run", "--", "diff3", "ours.txt", "base.txt", "theirs.txt"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout

    # Run the system diff3 with merge option
    system_diff3 = subprocess.run(
        ["diff3", "-m", "ours.txt", "base.txt", "theirs.txt"],
        capture_output=True,
        text=True,
    ).stdout

    return local_diff3, system_diff3

def main():
    """Main function."""
    merge_commits = get_merge_commits()
    print(f"Found {len(merge_commits)} merge commits.")
    comparison_count = 0
    for commit in merge_commits:
        conflicted_files = get_conflicted_files(commit)
        print(f"  Found {len(conflicted_files)} changed files in commit {commit}.")
        for filename in conflicted_files:
            if not filename:
                continue
            print(f"    Checking file: {filename} in commit {commit}...")
            file_versions = get_file_versions(commit, filename)
            if file_versions is None:
                print(f"      Skipping {filename}: Could not get all three file versions.")
                continue

            base_content, ours_content, theirs_content = file_versions
            if not base_content and not ours_content and not theirs_content:
                print(f"      Skipping {filename}: All three versions are empty.")
                continue

            if ours_content == base_content and theirs_content == base_content:
                print(f"      Skipping {filename}: All three versions are identical.")
                continue

            if ours_content == theirs_content:
                print(f"      Skipping {filename}: Ours and Theirs versions are identical.")
                continue

            print(f"      Performing diff3 comparison for {filename}...")
            local_diff3, system_diff3 = run_diff3(file_versions)
            comparison_count += 1

            if local_diff3 != system_diff3:
                print(f"  Outputs differ for {filename} in {commit}")
                dirname = os.path.join("diff_outputs", f"{filename.replace('/', '_')}_{commit}")
                os.makedirs(dirname, exist_ok=True)
                base, ours, theirs = file_versions
                with open(os.path.join(dirname, "base.txt"), "w") as f:
                    f.write(base)
                with open(os.path.join(dirname, "ours.txt"), "w") as f:
                    f.write(ours)
                with open(os.path.join(dirname, "theirs.txt"), "w") as f:
                    f.write(theirs)
                with open(os.path.join(dirname, "local_diff3.txt"), "w") as f:
                    f.write(local_diff3)
                with open(os.path.join(dirname, "system_diff3.txt"), "w") as f:
                    f.write(system_diff3)
                print(f"  Saved file versions and diff outputs to {dirname}")
    print(f"Total comparisons made: {comparison_count}")

if __name__ == "__main__":
    main()
