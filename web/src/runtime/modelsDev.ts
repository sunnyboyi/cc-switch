import { queryOptions } from "@tanstack/react-query";
import type { ModelsDevResponse } from "../../../src/lib/modelsDev";
import { command } from "./core";
export type * from "../../../src/lib/modelsDev";
export const MODELS_DEV_API_URL = "https://models.dev/api.json";
export function fetchModelsDev() {
  return command<ModelsDevResponse>("web_models_dev");
}
export const modelsDevQueryOptions = queryOptions({
  queryKey: ["models-dev"],
  queryFn: fetchModelsDev,
  staleTime: 60 * 60 * 1000,
  gcTime: 60 * 60 * 1000,
});
