// The shared frontend logger redacts payloads before reaching this browser sink.
export async function error(message: string) {
  console.error(message);
}
export async function warn(message: string) {
  console.warn(message);
}
export async function info(message: string) {
  console.info(message);
}
export async function debug(message: string) {
  console.debug(message);
}
