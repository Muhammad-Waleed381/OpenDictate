import { act, fireEvent, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useStore } from "@/lib/store";
import { useRecording } from "@/lib/useRecording";
import { useToastStore } from "@/components/ui/toast";
import { ModelCard } from "@/components/ModelCard";
import { Onboarding } from "@/components/Onboarding";
import { DockButton } from "@/components/DockButton";
import { MicTest } from "@/components/MicTest";
import { HomeTab } from "@/components/tabs/HomeTab";
import { HistoryTab } from "@/components/tabs/HistoryTab";
import { SnippetsTab } from "@/components/tabs/SnippetsTab";
import { SettingsTab } from "@/components/tabs/SettingsTab";
import { DockApp, MainApp } from "@/App";
import { deferred, model, settings, status } from "./fixtures";

const ipc = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: ipc.listen }));
const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();
let unlisteners: ReturnType<typeof vi.fn>[];
function emit(event: string, payload: unknown) {
  act(() => listeners.get(event)?.forEach((listener) => listener({ payload })));
}
function defaultInvoke(command: string) {
  switch (command) {
    case "get_settings": return Promise.resolve({ ...settings });
    case "models_status": return Promise.resolve({ ...status });
    case "models_catalog": return Promise.resolve(useStore.getState().catalog);
    case "get_mic": return Promise.resolve("mic");
    case "is_recording": return Promise.resolve(false);
    case "word_stats": return Promise.resolve(null);
    case "stop_recording": return Promise.resolve({ text: "", duration_ms: 0 });
    case "list_mics": return Promise.resolve([{ id: "mic", label: "Microphone" }]);
    case "get_history": case "get_dictionary": case "list_snippets": return Promise.resolve([]);
    default: return Promise.resolve();
  }
}
beforeEach(() => {
  useStore.setState(useStore.getInitialState(), true);
  useStore.setState({ settings: { ...settings }, models: { ...status }, hydrated: true,
    mics: [{ id: "mic", label: "Microphone" }], mic: "mic" });
  useToastStore.setState({ items: [] });
  listeners.clear();
  unlisteners = [];
  ipc.invoke.mockImplementation(defaultInvoke);
  ipc.listen.mockImplementation(async (event: string, callback: (event: { payload: unknown }) => void) => {
    const callbacks = listeners.get(event) ?? new Set();
    callbacks.add(callback);
    listeners.set(event, callbacks);
    const unlisten = vi.fn(() => callbacks.delete(callback));
    unlisteners.push(unlisten);
    return unlisten;
  });
});

describe("model lifecycle regressions", () => {
  it("B037 retains byte-complete progress until installation is acknowledged", () => {
    useStore.getState().addModelProgress({ file: "a", received: 100, total: 100 });
    expect(useStore.getState().modelProgress).toEqual([expect.objectContaining({ file: "a", received: 100 })]);
  });

  it("B037 ends a cached install with no progress event and refreshes readiness", async () => {
    useStore.setState({ catalog: [model("a")] });
    const install = deferred<void>();
    ipc.invoke.mockImplementation((command: string) => command === "ensure_model" ? install.promise : defaultInvoke(command));
    render(<ModelCard />);
    fireEvent.click(screen.getByRole("button", { name: "Download" }));
    expect(screen.getByText("Connecting…")).toBeInTheDocument();
    await act(async () => install.resolve());
    await waitFor(() => expect(screen.queryByText("Connecting…")).not.toBeInTheDocument());
    expect(ipc.invoke).toHaveBeenCalledWith("models_status");
    expect(ipc.invoke).toHaveBeenCalledWith("models_catalog");
  });

  it("B038 stops Download All after an installation failure", async () => {
    useStore.setState({ catalog: [model("a"), model("b")] });
    const install = deferred<void>();
    ipc.invoke.mockImplementation((command: string) => command === "ensure_model" ? install.promise : defaultInvoke(command));
    render(<ModelCard />);
    fireEvent.click(screen.getByRole("button", { name: /Download all missing/ }));
    await act(async () => install.reject(new Error("network failure")));
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 600)); });
    expect(ipc.invoke.mock.calls.filter(([command]) => command === "ensure_model")).toEqual([["ensure_model", { id: "a" }]]);
    expect(screen.queryByRole("button", { name: "Cancel all" })).not.toBeInTheDocument();
  });

  it("B037 refreshes status and catalog on models-ready", async () => {
    render(<DockApp />);
    await act(async () => {});
    emit("models-ready", { file: "a" });
    await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith("models_catalog"));
    expect(ipc.invoke).toHaveBeenCalledWith("models_status");
  });

  it("B039 does not advance onboarding for an unrelated installation", async () => {
    useStore.setState({ catalog: [model(settings.stt_model)], models: { ...status, stt_ready: true } });
    render(<Onboarding />);
    fireEvent.click(screen.getByRole("button", { name: "Next →" }));
    expect(screen.getByRole("button", { name: "Next →" })).toBeDisabled();
    emit("models-ready", { file: "caption-model" });
    await act(async () => {});
    expect(screen.queryByRole("button", { name: "Done" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next →" })).toBeDisabled();
  });

  it("B039 enables Next after explicitly selecting an installed STT model", async () => {
    let saved = { ...settings };
    useStore.setState({ catalog: [model(settings.stt_model), model("installed", { installed: true })] });
    ipc.invoke.mockImplementation((command: string, args?: { settings: Partial<typeof settings> }) => {
      if (command === "set_settings") { saved = { ...saved, ...args?.settings }; return Promise.resolve(); }
      if (command === "get_settings") return Promise.resolve(saved);
      return defaultInvoke(command);
    });
    render(<Onboarding />);
    fireEvent.click(screen.getByRole("button", { name: "Next →" }));
    expect(screen.getByRole("button", { name: "Next →" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Use" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Next →" })).toBeEnabled());
    expect(useStore.getState().settings?.stt_model).toBe("installed");
    fireEvent.click(screen.getByRole("button", { name: "Next →" }));
    expect(screen.getByRole("button", { name: "Done" })).toBeInTheDocument();
  });

  it("B040 allows selecting unmeasured streaming models despite a slow CPU benchmark", () => {
    useStore.setState({ catalog: [model("unmeasured", { installed: true, streaming: true })],
      models: { ...status, streaming_rtf_x100: 250 } });
    render(<ModelCard />);
    fireEvent.click(screen.getByRole("button", { name: "Live captions" }));
    expect(screen.getByRole("button", { name: "Use" })).toBeEnabled();
  });

  it("B040 scopes the CPU advisory to the measured Parakeet model and keeps GPU choices available", () => {
    useStore.setState({
      catalog: [model("parakeet-unified-en-0.6b-int8-streaming-560ms", { installed: true, streaming: true })],
      models: { ...status, streaming_rtf_x100: 250 },
    });
    render(<ModelCard />);
    fireEvent.click(screen.getByRole("button", { name: "Live captions" }));
    expect(screen.getByText(/Parakeet CPU benchmark/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use" })).toBeEnabled();
    act(() => useStore.setState({ models: { ...status, streaming_rtf_x100: 250, gpu_active: true } }));
    expect(screen.queryByText(/Parakeet CPU benchmark/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use" })).toBeEnabled();
  });

  it("B006 prevents a selected model change during recording", () => {
    useStore.setState({ recording: true, catalog: [model("other", { installed: true })] });
    render(<ModelCard />);
    expect(screen.getByRole("button", { name: "Use" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Delete" })).toBeDisabled();
  });

  it("B038 waits for extraction before starting the next Download All item", async () => {
    useStore.setState({ catalog: [model("a"), model("b")] });
    const first = deferred<void>();
    const second = deferred<void>();
    ipc.invoke.mockImplementation((command: string, args?: { id: string }) =>
      command === "ensure_model" ? (args?.id === "a" ? first.promise : second.promise) : defaultInvoke(command));
    render(<ModelCard />);
    fireEvent.click(screen.getByRole("button", { name: /Download all missing/ }));
    act(() => useStore.getState().addModelProgress({ file: "a", received: 100, total: 100 }));
    expect(screen.getByText("Installing…")).toBeInTheDocument();
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 600)); });
    expect(ipc.invoke.mock.calls.filter(([command]) => command === "ensure_model")).toHaveLength(1);
    await act(async () => first.resolve());
    expect(ipc.invoke).toHaveBeenCalledWith("ensure_model", { id: "b" });
    await act(async () => second.resolve());
    expect(screen.queryByRole("button", { name: "Cancel all" })).not.toBeInTheDocument();
  });

  it("B037 keeps cancellation busy until worker cleanup settles", async () => {
    useStore.setState({ catalog: [model("a"), model("b")] });
    const install = deferred<void>();
    ipc.invoke.mockImplementation((command: string) => command === "ensure_model" ? install.promise : defaultInvoke(command));
    render(<ModelCard />);
    fireEvent.click(screen.getByRole("button", { name: /Download all missing/ }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel all" }));
    await act(async () => {});
    expect(screen.getByRole("button", { name: "Cancelling…" })).toBeDisabled();
    expect(useStore.getState().modelDownloads).toEqual(["a"]);
    await act(async () => install.reject("Download cancelled"));
    expect(useStore.getState().modelDownloads).toEqual([]);
    expect(ipc.invoke.mock.calls.filter(([command]) => command === "ensure_model")).toHaveLength(1);
  });

  it("B037 preserves an installing model across ModelCard remounts", async () => {
    useStore.setState({ catalog: [model("a")] });
    const install = deferred<void>();
    ipc.invoke.mockImplementation((command: string) => command === "ensure_model" ? install.promise : defaultInvoke(command));
    const view = render(<ModelCard />);
    fireEvent.click(screen.getByRole("button", { name: "Download" }));
    view.unmount();
    render(<ModelCard />);
    expect(screen.getByRole("button", { name: "✕ Cancel" })).toBeEnabled();
    await act(async () => install.resolve());
    expect(screen.queryByText("Connecting…")).not.toBeInTheDocument();
  });

  it("B037 only declares KWS ready after completed installation and status refresh", async () => {
    useStore.setState({ settings: { ...settings, handsfree_mode: true } });
    const install = deferred<void>();
    let ready = false;
    ipc.invoke.mockImplementation((command: string) => {
      if (command === "ensure_model") return install.promise;
      if (command === "models_status") return Promise.resolve({ ...status, kws_ready: ready });
      return defaultInvoke(command);
    });
    render(<SettingsTab />);
    fireEvent.click(screen.getByRole("button", { name: "Download Model (15MB)" }));
    act(() => useStore.getState().addModelProgress({ file: "kws-zipformer-gigaspeech-3.3m-2024-01-01", received: 100, total: 100 }));
    expect(screen.getByRole("button", { name: "Downloading..." })).toBeDisabled();
    expect(useToastStore.getState().items.some((item) => item.kind === "success")).toBe(false);
    ready = true;
    await act(async () => install.resolve());
    expect(useStore.getState().models?.kws_ready).toBe(true);
    expect(useStore.getState().modelProgress).toEqual([]);
    expect(useToastStore.getState().items.some((item) => item.message.includes("downloaded and ready"))).toBe(true);
  });
});

describe("session and listener regressions", () => {
  it("B006 shares the recording lock between hook instances", async () => {
    const start = deferred<void>();
    ipc.invoke.mockImplementation((command: string) => command === "start_recording" ? start.promise : defaultInvoke(command));
    const first = renderHook(useRecording);
    const second = renderHook(useRecording);
    act(() => { void first.result.current.toggle(); void second.result.current.toggle(); });
    expect(ipc.invoke.mock.calls.filter(([command]) => command === "start_recording")).toHaveLength(1);
    await act(async () => start.resolve());
  });

  it("B006 does not overwrite newer recording events with a pending start response", async () => {
    const start = deferred<void>();
    ipc.invoke.mockImplementation((command: string) => command === "start_recording" ? start.promise : defaultInvoke(command));
    const hook = renderHook(useRecording);
    act(() => { void hook.result.current.toggle(); });
    act(() => { useStore.getState().setRecording(true); useStore.getState().setRecording(false); });
    await act(async () => start.resolve());
    expect(useStore.getState().recording).toBe(false);
  });

  it("B006 keeps a native recording event newer than refreshAll's snapshot", async () => {
    const snapshot = deferred<boolean>();
    ipc.invoke.mockImplementation((command: string) => command === "is_recording" ? snapshot.promise : defaultInvoke(command));
    const refresh = useStore.getState().refreshAll();
    await act(async () => {});
    act(() => useStore.getState().setRecording(true));
    await act(async () => { snapshot.resolve(false); await refresh; });
    expect(useStore.getState().recording).toBe(true);
  });

  it("B006 ignores an old stop result after a newer native session has opened and closed", async () => {
    useStore.getState().setRecording(true);
    const stop = deferred<{ text: string; duration_ms: number }>();
    ipc.invoke.mockImplementation((command: string) => command === "stop_recording" ? stop.promise : defaultInvoke(command));
    const hook = renderHook(useRecording);
    act(() => { void hook.result.current.toggle(); });
    act(() => {
      useStore.getState().setRecording(false);
      useStore.getState().setRecording(true);
      useStore.getState().setRecording(false);
    });
    await act(async () => stop.resolve({ text: "old session", duration_ms: 10 }));
    expect(useStore.getState().lastResult).toBeNull();
  });

  it("B001 refuses a microphone test while dictation is active", () => {
    useStore.getState().setRecording(true);
    render(<MicTest />);
    expect(screen.getByRole("button", { name: "● Start" })).toBeDisabled();
  });

  it("B001 performs a microphone-only test without publishing a dictation result", async () => {
    ipc.invoke.mockImplementation((command: string) => command === "stop_recording"
      ? Promise.resolve({ text: "Microphone detected audio", duration_ms: 100 }) : defaultInvoke(command));
    render(<MicTest />);
    fireEvent.click(screen.getByRole("button", { name: "● Start" }));
    await act(async () => {});
    expect(ipc.invoke).toHaveBeenCalledWith("start_recording", { mode: "test" });
    expect(ipc.invoke).not.toHaveBeenCalledWith("cancel_recording");
    fireEvent.click(screen.getByRole("button", { name: "■ Stop" }));
    await act(async () => {});
    expect(useStore.getState().lastResult).toBeNull();
    expect(screen.getByRole("button", { name: "● Start" })).toBeEnabled();
  });

  it("B006 resets the mic-test control when a native cancellation closes the session", async () => {
    render(<MicTest />);
    fireEvent.click(screen.getByRole("button", { name: "● Start" }));
    await act(async () => {});
    act(() => useStore.getState().setRecording(false));
    expect(screen.getByRole("button", { name: "● Start" })).toBeEnabled();
  });

  it("B006 does not let an old mic-test control stop a new dictation session", async () => {
    render(<MicTest />);
    fireEvent.click(screen.getByRole("button", { name: "● Start" }));
    await act(async () => {});
    act(() => { useStore.getState().setRecording(false); useStore.getState().setRecording(true); });
    expect(screen.getByRole("button", { name: "● Start" })).toBeDisabled();
  });

  it("B006 cancels a pending mic-test start on unmount without restoring stale recording state", async () => {
    const start = deferred<void>();
    ipc.invoke.mockImplementation((command: string) => command === "start_recording" ? start.promise : defaultInvoke(command));
    const view = render(<MicTest />);
    fireEvent.click(screen.getByRole("button", { name: "● Start" }));
    view.unmount();
    await act(async () => {});
    expect(ipc.invoke).toHaveBeenCalledWith("cancel_recording");
    await act(async () => start.resolve());
    expect(useStore.getState().recording).toBe(false);
  });

  it("B054 unregisters all native subscriptions resolved before unmount", async () => {
    const view = render(<DockApp />);
    await act(async () => {});
    view.unmount();
    expect(unlisteners.length).toBeGreaterThan(5);
    for (const unlisten of unlisteners) expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("B054 unregisters subscriptions resolving after unmount", async () => {
    const registration = deferred<() => void>();
    const unlisten = vi.fn();
    ipc.listen.mockReturnValue(registration.promise);
    const view = render(<DockApp />);
    view.unmount();
    await act(async () => registration.resolve(unlisten));
    expect(unlisten).toHaveBeenCalledTimes(ipc.listen.mock.calls.length);
  });
});

describe("interaction regressions", () => {
  it.each([
    { text: "copy-only B", injected: false, copied: true },
    { text: "failed B", injected: false, error: "No focused target" },
  ])("B031/B050 disables Undo after inserted A is followed by $text", async (payload) => {
    window.history.replaceState(null, "", "/");
    render(<MainApp />);
    await act(async () => {});
    emit("transcript", { text: "inserted A", injected: true, copied: false });
    expect(screen.getByRole("button", { name: "Undo" })).toBeEnabled();
    emit("transcript", payload);
    expect(screen.getByText(`“${payload.text}”`)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Undo" })).toBeDisabled();
    expect(screen.queryByText("INSERTED ✓")).not.toBeInTheDocument();
    expect(useStore.getState().lastResult).toMatchObject(payload);
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(ipc.invoke).not.toHaveBeenCalledWith("undo_last_insert");
  });

  it("B031/B050 requires insertion confirmation before allowing Undo for a stop response", async () => {
    useStore.getState().setRecording(true);
    ipc.invoke.mockImplementation((command: string) => command === "stop_recording"
      ? Promise.resolve({ text: "unconfirmed result", duration_ms: 100 }) : defaultInvoke(command));
    render(<HomeTab />);
    fireEvent.click(screen.getAllByRole("button", { name: "■ Stop" })[0]);
    await act(async () => {});
    expect(screen.getByText("“unconfirmed result”")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Undo" })).toBeDisabled();
  });

  it("B029 saves the explicit shell target profile", async () => {
    render(<SettingsTab />);
    const target = screen.getByRole("combobox", { name: "Input target" });
    expect(target).toHaveTextContent("Text fields");
    fireEvent.click(target);
    const shell = await screen.findByRole("option", { name: "Shell (Bash, Zsh)" });
    fireEvent.pointerDown(shell, { pointerType: "mouse" });
    fireEvent.click(shell);
    await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith("set_settings", { settings: { input_target: "terminal" } }));
    expect(useStore.getState().settings).toMatchObject({ input_target: "terminal" });
    expect(screen.getByText(/custom or vi key maps may differ/)).toBeInTheDocument();
  });

  it("B049 clears the success flash when the next recording begins", () => {
    useStore.setState({ overlayState: { state: "inserted" } });
    render(<DockButton />);
    act(() => useStore.getState().setOverlayState({ state: "recording" }));
    expect(screen.getByRole("button", { name: "Stop recording" })).not.toHaveClass("ring-emerald-400");
  });

  it("B050 enables Undo for a new result even when its text is identical", async () => {
    useStore.setState({ lastResult: { text: "same text", duration_ms: 0, injected: true } });
    render(<HomeTab />);
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Undone" })).toBeDisabled());
    act(() => useStore.setState({ lastResult: { text: "same text", duration_ms: 0, injected: true } }));
    expect(screen.getByRole("button", { name: "Undo" })).toBeEnabled();
  });

  it("B050 keeps Undo available after a refused native handoff", async () => {
    useStore.setState({ lastResult: { text: "text", duration_ms: 0, injected: true } });
    ipc.invoke.mockImplementation((command: string) => command === "undo_last_insert" ? Promise.reject("No external target") : defaultInvoke(command));
    render(<HomeTab />);
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    await act(async () => {});
    expect(screen.getByRole("button", { name: "Undo" })).toBeEnabled();
  });

  it("B051 derives Select all from visible IDs and preserves hidden selections", () => {
    useStore.setState({ history: [1, 2, 3].map((id) => ({ id, text: id === 1 ? "hidden" : "visible", created_at: "1", duration_ms: 0, source: "dictate" })) });
    render(<HistoryTab />);
    fireEvent.click(screen.getByRole("checkbox", { name: "Select dictation 1" }));
    fireEvent.change(screen.getByPlaceholderText(/Search/), { target: { value: "visible" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Select dictation 2" }));
    expect(screen.getByRole("checkbox", { name: "Select all dictations" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Select all dictations" })).toBePartiallyChecked();
    fireEvent.click(screen.getByRole("checkbox", { name: "Select all dictations" }));
    expect(screen.getByRole("checkbox", { name: "Select dictation 3" })).toBeChecked();
    fireEvent.change(screen.getByPlaceholderText(/Search/), { target: { value: "" } });
    expect(screen.getByRole("checkbox", { name: "Select dictation 1" })).toBeChecked();
  });

  it("B052 sends Test Insert through native snippet expansion", async () => {
    useStore.setState({ snippets: [{ id: 1, trigger: "test", text: "{date}: {clipboard}", created_at: "1" }] });
    render(<SnippetsTab />);
    fireEvent.click(screen.getByTitle(/Insert into|Test Insert/));
    await waitFor(() => expect(ipc.invoke).toHaveBeenCalledWith("test_snippet", { text: "{date}: {clipboard}" }));
  });

  it("B025 shows saved Local SLM as unavailable with no active polish mode", () => {
    useStore.setState({ settings: { ...settings, polish_provider: "local_slm" } });
    render(<SettingsTab />);
    expect(screen.getByText(/Local SLM is unavailable/)).toBeInTheDocument();
    expect(screen.queryByText("Polish Output Mode")).not.toBeInTheDocument();
  });

  it("B025 leaves Local SLM disabled in the provider menu", async () => {
    render(<SettingsTab />);
    const polish = screen.getByText("✨ AI Voice Polish").closest("div.rounded-none")!;
    fireEvent.click(within(polish as HTMLElement).getByRole("combobox"));
    const option = await screen.findByRole("option", { name: /Local SLM/ });
    expect(option).toHaveAttribute("aria-disabled", "true");
  });
});
