import agentbox

def test_sum():
    # Test the Rust binding
    from agentbox import _core
    result = _core.sum_as_string(10, 20)
    assert result == "30"
    print(f"Core sum test passed: {result}")
