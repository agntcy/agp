# MCP Test

1. Run mcpfast server
```
$ git clone git@github.com:modelcontextprotocol/python-sdk.git
$ cd python-sdk/
$ uv venv
$ uv pip install fastmcp
$ cd examples/fastmcp/
$ uv run fastmcp run -t sse echo.py
```

2. Run agp
```
task run:agp
```

3. Run MCP Server
```
task run:run:mcp:server
```

4. Run MCP Client
```
task run:run:mcp:client
```