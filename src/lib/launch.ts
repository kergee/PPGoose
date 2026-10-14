import { listen } from "@tauri-apps/api/event";
import { tauriApi } from "./tauri";
import { useStore } from "../store/useStore";

/** Subscribe before draining the backend queue so startup events cannot be lost. */
export function subscribeLaunchPaths(): () => void {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  const drain = async () => {
    try {
      const paths = await tauriApi.takeLaunchPaths();
      if (paths.length) await useStore.getState().addPaths(paths);
    } catch (error) { useStore.setState({ message: `添加文件失败：${String(error)}` }); }
  };
  listen("shell://paths", () => { void drain(); }).then((stop) => {
    if (disposed) { stop(); return; }
    unlisten = stop;
    void drain();
  }).catch((error) => { useStore.setState({ message: String(error) }); });
  return () => { disposed = true; unlisten?.(); };
}
