import { command } from "./core";
export function homeDir() {
  return command<string>("web_home_dir");
}
export function join(...parts: string[]) {
  return command<string>("web_join_path", { parts });
}
