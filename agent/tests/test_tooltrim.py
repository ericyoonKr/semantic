from semantic_agent.agent.tooltrim import UNUSED_TOOLS, trimmed


class Named:
    def __init__(self, name: str) -> None:
        self.name = name


def test_unused_file_and_todo_tools_are_trimmed_but_reading_and_delegation_stay() -> None:
    tools = [Named(n) for n in ["ls", "read_file", "write_file", "edit_file", "delete", "glob", "grep", "execute", "write_todos", "task", "judge", "search_document"]]
    kept = [t.name for t in trimmed(tools, UNUSED_TOOLS)]
    assert kept == ["read_file", "task", "judge", "search_document"]
    assert [t["name"] for t in trimmed([{"name": "grep"}, {"name": "task"}], UNUSED_TOOLS)] == ["task"]
