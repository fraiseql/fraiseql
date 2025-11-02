# Extracted from: docs/advanced/nested-array-filtering.md
# Block number: 3
async def test_complex_filtering():
    # Create complex filter conditions
    windows_condition = PrintServerWhereInput()
    windows_condition.operating_system = {"equals": "Windows Server"}
    windows_condition.nTotalAllocations = {"gte": 100}

    linux_condition = PrintServerWhereInput()
    linux_condition.operating_system = {"equals": "Linux"}
    linux_condition.ipAddress = {"isnull": False}

    # Combine with OR
    where_filter = PrintServerWhereInput()
    where_filter.OR = [windows_condition, linux_condition]

    # Execute filtering
    result = await resolver(network_config, None, where=where_filter)

    # Process results
    for server in result:
        print(f"Found: {server.hostname} ({server.operating_system})")
