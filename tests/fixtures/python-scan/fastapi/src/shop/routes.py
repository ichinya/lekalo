from fastapi import APIRouter as Router
from .handlers import list_items

router = Router(prefix="/v1")
router.add_api_route("/items", list_items, methods=["GET"], operation_id="list_items")
