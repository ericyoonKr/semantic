import type { ApiClient } from "../../shared/api/client";

export async function deletePrefs(client: ApiClient): Promise<void> {
  await client.empty("DELETE", "/prefs");
}
