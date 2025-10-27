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
*   **Dependency Injection**: Prefer dependency injection over global state or direct instantiation of dependencies. This improves modularity, testability, and flexibility.

#### **Code Style**

*   Except as given in this document, code should follow PEP8 style.
*   **Line Length**: Wrap lines at 80 characters, preferably using parentheses.
*   **Conditionals**:
    *   Implicit boolean checks (`if foo`) should only be used for variables that are either `True` or `False`.
    *   For all other types, checks must be explicit:
        *   **`None`**: Use `if foo is not None` or `if foo is None`.
        *   **Containers (lists, dicts, etc.)**: Use `if len(foo) > 0` or `if len(foo) == 0`.
        *   **Strings**: Use `if foo != ''` or `if foo == ''`.
        *   **Numbers**: Use `if foo != 0`.
        *   in unit tests, when asserting the return value of a callable under
            test, use self.assertIs() for booleans.
*   **Boolean Arguments**: When providing boolean arguments, always use keyword arguments to provide context about their meaning. Position boolean arguments after any typically positional arguments.
*   **String Splitting**: Always use `str.splitlines()` to split lines.
*   **Quotes**: Use single-quotes (`'`) for strings, unless a string contains single-quotes, in which case double-quotes (`"`) should be used to avoid escapes.
* Lines, especially otherwise-blank lines, should not have tailing whitespace.

#### **Data Structures**

*   **Choose Appropriately**: Use the most appropriate data structure for the task. For fixed-size, immutable collections, prefer tuples over lists. When writing a container literal in order to use `in` or `not in`, write a set literal.
*   **Path Manipulation**: Always use `pathlib.Path` for file system paths.
*   **Static Methods**: static methods of a class should never refer to that class by name.  Instead, they should be converted into class methods and should use `cls` to refer to their class.  Methods that do not refer to instance state should be written as staticmethods or classmethods.
*   **Stateless classes**: Classes that have no state should be used directly, not instantiated.

#### **Error Handling & I/O**

*   **Minimize `try...except` blocks**: The `try` block should only contain the code that can raise the specific exception you are handling.
*   **External Command Exit Codes**: Be mindful of their specific exit codes. Handle expected non-zero exit codes (e.g., `diff` returning 1) gracefully, instead of treating them all as errors.
*   **Output Streams**: Write all error messages and progress messages to `stderr`. If the `logging` module is already in use, use it.

#### **Testing**

*   **Test Structure**: Organize unit tests into separate `unittest.TestCase` classes for each function or class under test. This improves test isolation and discoverability.
*   **Dependency Injection in Tests**: Use dependency injection in tests by passing mock objects as dependencies, rather than relying on global patching, to make tests more explicit and robust.
