# Extracted from: docs/rust/RUST_PIPELINE_IMPLEMENTATION_GUIDE.md
# Block number: 18
from fastapi import FastAPI

from fraiseql.fastapi import make_graphql_app
from fraiseql.fastapi.response_handlers import handle_graphql_response

app = FastAPI()
graphql_app = make_graphql_app()


@app.post("/graphql")
async def graphql_endpoint(request):
    result = await graphql_app.execute(request)
    return handle_graphql_response(result)  # Automatic RustResponseBytes handling
