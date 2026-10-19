import { useStore } from "@/lib/store";
import { toast } from "@/components/ui/toast";

export function useRecording() {
  const recording = useStore((s) => s.recording);
  const busy = useStore((s) => s.recordingBusy || s.recordingMode === "test");

  const toggle = async () => {
    const store = useStore.getState();
    if (store.recordingBusy || store.recordingMode === "test") return;
    try {
      if (store.recording) await store.stopRecording();
      else await store.startRecording("dictate");
    } catch (e) {
      toast.error(`${store.recording ? "Stop failed" : "Could not start recording"}: ${String(e)}`);
    }
  };

  return { recording, busy, toggle };
}
