import unittest
from unittest.mock import MagicMock, call, patch
import io
import sys
from pathlib import Path

import compare_diff3

class TestGetConflictedFiles(unittest.TestCase):

    def test_basic_conflict(self):
        # Arrange
        mock_strategy = MagicMock()
        mock_strategy.get_merge_base.return_value = 'merge_base_hash'
        mock_strategy._get_changed_files.side_effect = [
            {'file1.txt', 'file2.txt'},  # for parent1
            {'file2.txt', 'file3.txt'},  # for parent2
        ]

        # Act
        conflicted_files = compare_diff3.get_conflicted_files(mock_strategy, 'some_commit')

        # Assert
        self.assertEqual(conflicted_files, ['file2.txt'])
        mock_strategy.get_merge_base.assert_called_once_with('some_commit')
        mock_strategy._get_changed_files.assert_has_calls([
            call('merge_base_hash', 'some_commit^1'),
            call('merge_base_hash', 'some_commit^2'),
        ])

class TestGetAndFilterFileVersions(unittest.TestCase):

    @patch('sys.stderr', new_callable=io.StringIO)
    def test_identical_all_versions(self, mock_stderr):
        # Arrange
        mock_strategy = MagicMock()
        mock_strategy.get_merge_base.return_value = 'merge_base_hash'
        mock_strategy._get_file_version.side_effect = [
            'content', # ours
            'content', # base
            'content', # theirs
        ]

        # Act
        result = compare_diff3._get_and_filter_file_versions(mock_strategy, 'commit', 'file.txt')

        # Assert
        self.assertIsNone(result)
        self.assertIn('Skipping file.txt: All three versions are identical', mock_stderr.getvalue())
        mock_strategy.get_merge_base.assert_called_once_with('commit')
        mock_strategy._get_file_version.assert_has_calls([
            call('commit^1', 'file.txt'),
            call('merge_base_hash', 'file.txt'),
            call('commit^2', 'file.txt'),
        ])

    @patch('sys.stderr', new_callable=io.StringIO)
    def test_identical_ours_theirs_versions(self, mock_stderr):
        # Arrange
        mock_strategy = MagicMock()
        mock_strategy.get_merge_base.return_value = 'merge_base_hash'
        mock_strategy._get_file_version.side_effect = [
            'content', # ours
            'different_content', # base
            'content', # theirs
        ]

        # Act
        result = compare_diff3._get_and_filter_file_versions(mock_strategy, 'commit', 'file.txt')

        # Assert
        self.assertIsNone(result)
        self.assertIn('Skipping file.txt: Ours and Theirs versions are identical', mock_stderr.getvalue())
        mock_strategy.get_merge_base.assert_called_once_with('commit')
        mock_strategy._get_file_version.assert_has_calls([
            call('commit^1', 'file.txt'),
            call('merge_base_hash', 'file.txt'),
            call('commit^2', 'file.txt'),
        ])

    @patch('sys.stderr', new_callable=io.StringIO)
    def test_no_file_versions_found(self, mock_stderr):
        # Arrange
        mock_strategy = MagicMock()
        mock_strategy.get_merge_base.return_value = 'merge_base_hash'
        mock_strategy._get_file_version.side_effect = [
            '', # ours
            '', # base
            '', # theirs
        ]

        # Act
        result = compare_diff3._get_and_filter_file_versions(mock_strategy, 'commit', 'file.txt')

        # Assert
        self.assertIsNone(result)
        self.assertIn('Skipping file.txt: Could not get all three file versions', mock_stderr.getvalue())
        mock_strategy.get_merge_base.assert_called_once_with('commit')
        mock_strategy._get_file_version.assert_has_calls([
            call('commit^1', 'file.txt'),
            call('merge_base_hash', 'file.txt'),
            call('commit^2', 'file.txt'),
        ])

class TestRunDiff3Commands(unittest.TestCase):

    def test_basic_run(self):
        # Arrange
        mock_executor = MagicMock(return_value='mock_diff_output')
        
        paths = compare_diff3.FileVersions(
            ours='path/to/ours.txt',
            base='path/to/base.txt',
            theirs='path/to/theirs.txt'
        )

        # Act
        local_diff, system_diff = compare_diff3._run_diff3_commands(paths, executor=mock_executor)

        # Assert
        self.assertEqual(local_diff, 'mock_diff_output')
        self.assertEqual(system_diff, 'mock_diff_output')

        mock_executor.assert_has_calls([
            call(paths, is_local=True),
            call(paths, is_local=False),
        ], any_order=False)

class TestRunDiff3CommandExecutor(unittest.TestCase):

    @patch('subprocess.run')
    @patch('sys.stderr', new_callable=io.StringIO)
    def test_local_command_execution(self, mock_stderr, mock_subprocess_run):
        # Arrange
        mock_process = MagicMock()
        mock_process.returncode = 0
        mock_process.stdout = 'local_output'
        mock_subprocess_run.return_value = mock_process

        paths = compare_diff3.FileVersions(
            ours='path/to/ours.txt',
            base='path/to/base.txt',
            theirs='path/to/theirs.txt'
        )

        # Act
        result = compare_diff3._run_diff3_command_executor(paths, is_local=True)

        # Assert
        self.assertEqual(result, 'local_output')
        expected_cmd = ['cargo', 'run', '--', 'diff3', 'path/to/ours.txt', 'path/to/base.txt', 'path/to/theirs.txt']
        mock_subprocess_run.assert_called_once_with(
            expected_cmd, capture_output=True, text=True
        )
        self.assertEqual(mock_stderr.getvalue(), '') # No error output

    @patch('subprocess.run')
    @patch('sys.stderr', new_callable=io.StringIO)
    def test_system_command_execution(self, mock_stderr, mock_subprocess_run):
        # Arrange
        mock_process = MagicMock()
        mock_process.returncode = 0
        mock_process.stdout = 'system_output'
        mock_subprocess_run.return_value = mock_process

        paths = compare_diff3.FileVersions(
            ours='path/to/ours.txt',
            base='path/to/base.txt',
            theirs='path/to/theirs.txt'
        )

        # Act
        result = compare_diff3._run_diff3_command_executor(paths, is_local=False)

        # Assert
        self.assertEqual(result, 'system_output')
        expected_cmd = ['diff3', '-m', 'path/to/ours.txt', 'path/to/base.txt', 'path/to/theirs.txt']
        mock_subprocess_run.assert_called_once_with(
            expected_cmd, capture_output=True, text=True
        )
        self.assertEqual(mock_stderr.getvalue(), '') # No error output

    @patch('subprocess.run')
    @patch('sys.stderr', new_callable=io.StringIO)
    def test_command_error_handling(self, mock_stderr, mock_subprocess_run):
        # Arrange
        mock_process = MagicMock()
        mock_process.returncode = 2 # Not 0 or 1
        mock_process.stdout = ''
        mock_process.stderr = 'error_message'
        mock_subprocess_run.return_value = mock_process

        paths = compare_diff3.FileVersions(
            ours='path/to/ours.txt',
            base='path/to/base.txt',
            theirs='path/to/theirs.txt'
        )

        # Act
        result = compare_diff3._run_diff3_command_executor(paths, is_local=True)

        # Assert
        self.assertEqual(result, '') # Returns empty string on error
        self.assertIn('Error running command:', mock_stderr.getvalue())
        self.assertIn('error_message', mock_stderr.getvalue())

class TestGitStrategy(unittest.TestCase):

    @patch('compare_diff3.GitStrategy._run_git_command') # Mock the classmethod _run_git_command
    def test_get_merge_commits(self, mock_run_git_command):
        # Arrange
        mock_run_git_command.return_value = 'commit1\ncommit2'

        # Act
        commits = compare_diff3.GitStrategy.get_merge_commits()

        # Assert
        self.assertEqual(commits, ['commit1', 'commit2'])
        mock_run_git_command.assert_called_once_with(['log', '--merges', '--pretty=%H'])

    @patch('compare_diff3.GitStrategy._run_git_command')
    def test_get_changed_files(self, mock_run_git_command):
        # Arrange
        mock_run_git_command.return_value = 'fileA.txt\nfileB.txt'

        # Act
        files = compare_diff3.GitStrategy._get_changed_files('ref1', 'ref2')

        # Assert
        self.assertEqual(files, {'fileA.txt', 'fileB.txt'})
        mock_run_git_command.assert_called_once_with(['diff', '--name-only', 'ref1', 'ref2'])

    @patch('compare_diff3.GitStrategy._run_git_command')
    def test_get_merge_base(self, mock_run_git_command):
        # Arrange
        mock_run_git_command.return_value = 'merge_base_hash'

        # Act
        merge_base = compare_diff3.GitStrategy.get_merge_base('commit')

        # Assert
        self.assertEqual(merge_base, 'merge_base_hash')
        mock_run_git_command.assert_called_once_with(['merge-base', 'commit^1', 'commit^2'])

    @patch('compare_diff3.GitStrategy._run_git_command')
    def test_get_file_version(self, mock_run_git_command):
        # Arrange
        mock_run_git_command.return_value = 'file_content'

        # Act
        content = compare_diff3.GitStrategy._get_file_version('ref', 'file.txt')

        # Assert
        self.assertEqual(content, 'file_content')
        mock_run_git_command.assert_called_once_with(['show', 'ref:file.txt'], check_result=False)

if __name__ == '__main__':
    unittest.main()
