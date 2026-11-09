import fastapi as api
from .routes import router

app = api.FastAPI()
app.include_router(router, prefix="/api")
direct = api.APIRouter()
from .handlers import list_items
direct.add_api_route("/items", list_items)
app.include_router(direct, prefix="/public")
