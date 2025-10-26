When I ask you to re-read a file, always re-read it.

### **PYTHON**

#### **General Principles**

*   **Be DRY (Don't Repeat Yourself)**: Aggressively refactor to avoid code duplication.
*   **Separation of Concerns**: Functions should have a single, well-defined responsibility. The caller is generally responsible for the lifecycle (e.g., creation, cleanup) of the objects it passes to a function.
*   **Readability**: Write code that is clear and easy to understand.

#### **Refactoring Guidelines**

*   **Extract Functions**: Refactor functions that are longer than 25 lines. Extracted functions should be at least 5 statements long.
*   **Consolidate Repeated Calls**: If you are calling the same method multiple times in a row, refactor it into a loop.
*   **Use Dataclasses for Arguments**: If you are passing the same set of arguments to multiple functions, group them into a `dataclass` to make the code more explicit and self-documenting.
*   **Use Generators for Data Pipelines**: Use generators to separate the logic of producing a sequence of items from the logic of consuming them. Filtering logic should be encapsulated within the generator.

#### **Code Style**

*   **Line Length**: Wrap lines at 80 characters, preferably using parentheses.
*   **Conditionals**: Use `if foo` only for boolean values. Do not use it to check for `None`, empty containers, or empty strings.
*   **String Splitting**: Always use `str.splitlines()` to split lines.
*   **Quotes**: Use single-quotes (`'`) for strings, unless a string contains single-quotes, in which case double-quotes (`"`) should be used to avoid escapes.

#### **Data Structures**

*   **Choose Appropriately**: Use the most appropriate data structure for the task. For fixed-size, immutable collections, prefer tuples over lists.
*   **Path Manipulation**: Always use `pathlib.Path` for file system paths.

#### **Error Handling & I/O**

*   **Minimize `try...except` blocks**: The `try` block should only contain the code that can raise the specific exception you are handling.
*   **External Command Exit Codes**: Be mindful of the specific exit codes of external commands. Handle expected non-zero exit codes (e.g., `diff` returning 1) gracefully, instead of treating them all as errors.
*   **Output Streams**: Write all error messages and progress indicators to `stderr`. If the `logging` module is already in use, use it.
