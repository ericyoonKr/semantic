from langgraph.checkpoint.base import BaseCheckpointSaver
from langgraph.checkpoint.postgres.aio import AsyncPostgresSaver
from langgraph.store.base import BaseStore
from langgraph.store.postgres.aio import AsyncPostgresStore
from psycopg.rows import dict_row
from psycopg_pool import AsyncConnectionPool

from semantic_agent.config import Settings

PREFS_FILE = "/answer_style.md"


def prefs_namespace(user_id: str) -> tuple[str, str, str]:
    return ("user", user_id, "prefs")


async def open_pool(settings: Settings) -> AsyncConnectionPool:
    pool = AsyncConnectionPool(
        settings.checkpoint_pg_url.get_secret_value(),
        max_size=settings.pg_pool_max,
        kwargs={"autocommit": True, "prepare_threshold": 0, "row_factory": dict_row},
        open=False,
    )
    await pool.open()
    return pool


async def open_checkpointer(pool: AsyncConnectionPool) -> AsyncPostgresSaver:
    saver = AsyncPostgresSaver(conn=pool)
    await saver.setup()
    return saver


async def open_store(pool: AsyncConnectionPool) -> AsyncPostgresStore:
    store = AsyncPostgresStore(conn=pool)
    await store.setup()
    return store


async def delete_session(saver: BaseCheckpointSaver, session_id: str) -> None:
    await saver.adelete_thread(session_id)


async def delete_prefs(store: BaseStore, user_id: str) -> None:
    namespace = prefs_namespace(user_id)
    while True:
        items = await store.asearch(namespace, limit=100)
        if not items:
            return
        for item in items:
            await store.adelete(namespace, item.key)
