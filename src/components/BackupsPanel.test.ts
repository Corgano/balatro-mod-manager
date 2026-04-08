import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import BackupsPanel from "./BackupsPanel.svelte";

const mockBackups = vi.hoisted(() => {
  type BackupState = {
    backups: Array<{
      id: string;
      created_at: string;
      trigger: "manual" | "auto_update" | "auto_uninstall" | "auto_bulk";
      name: string | null;
      mod_count: number;
      size_bytes: number;
      lovely_version: string | null;
    }>;
    loading: boolean;
    totalSize: number;
    error: string | null;
  };

  const defaultState: BackupState = {
    backups: [],
    loading: false,
    totalSize: 0,
    error: null,
  };

  let state: BackupState = { ...defaultState };
  const subscribers = new Set<(value: BackupState) => void>();

  const subscribe = (run: (value: BackupState) => void) => {
    run(state);
    subscribers.add(run);
    return () => subscribers.delete(run);
  };

  const setState = (next: BackupState) => {
    state = next;
    subscribers.forEach((run) => run(state));
  };

  const reset = () => {
    setState({ ...defaultState });
  };

  return {
    subscribe,
    setState,
    reset,
    loadMock: vi.fn(),
    getBackupsDirectoryMock: vi.fn(),
  };
});

const mockPopup = vi.hoisted(() => {
  let state = { visible: false };
  const subscribers = new Set<(value: { visible: boolean }) => void>();

  const subscribe = (run: (value: { visible: boolean }) => void) => {
    run(state);
    subscribers.add(run);
    return () => subscribers.delete(run);
  };

  const set = (next: { visible: boolean }) => {
    state = next;
    subscribers.forEach((run) => run(state));
  };

  const get = () => state;

  const reset = () => {
    set({ visible: false });
  };

  return { subscribe, set, get, reset };
});

const invokeMock = vi.hoisted(() => vi.fn());
const addMessageMock = vi.hoisted(() => vi.fn());

vi.mock("../stores/backups", () => ({
  backupsStore: {
    subscribe: mockBackups.subscribe,
    load: mockBackups.loadMock,
    getBackupsDirectory: mockBackups.getBackupsDirectoryMock,
  },
  formatBytes: (bytes: number) => `${bytes} B`,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("$lib/stores", () => ({
  addMessage: (...args: unknown[]) => addMessageMock(...args),
}));

vi.mock("../stores/modStore", () => ({
  createBackupPopupStore: {
    subscribe: mockPopup.subscribe,
    set: mockPopup.set,
  },
}));

describe("BackupsPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockBackups.reset();
    mockPopup.reset();
  });

  it("loads backups on mount", async () => {
    render(BackupsPanel);

    await waitFor(() => {
      expect(mockBackups.loadMock).toHaveBeenCalledTimes(1);
    });
  });

  it("opens backups folder when a directory is available", async () => {
    mockBackups.getBackupsDirectoryMock.mockResolvedValue("/tmp/backups");
    render(BackupsPanel);

    await fireEvent.click(
      screen.getByRole("button", { name: /open backups folder/i }),
    );

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("open_directory", {
        path: "/tmp/backups",
      });
    });
  });

  it("shows an error message when backups directory is unavailable", async () => {
    mockBackups.getBackupsDirectoryMock.mockResolvedValue(null);
    render(BackupsPanel);

    await fireEvent.click(
      screen.getByRole("button", { name: /open backups folder/i }),
    );

    await waitFor(() => {
      expect(addMessageMock).toHaveBeenCalledWith(
        "Failed to get backups directory",
        "error",
      );
    });
  });

  it("opens the create-backup popup when create button is clicked", async () => {
    render(BackupsPanel);

    await fireEvent.click(
      screen.getByRole("button", { name: /create backup/i }),
    );

    expect(mockPopup.get()).toEqual({ visible: true });
  });

  it("renders loading state", () => {
    mockBackups.setState({
      backups: [],
      loading: true,
      totalSize: 0,
      error: null,
    });
    render(BackupsPanel);
    expect(screen.getByText("Loading backups...")).toBeTruthy();
  });

  it("renders error state", () => {
    mockBackups.setState({
      backups: [],
      loading: false,
      totalSize: 0,
      error: "Failed to list backups",
    });
    render(BackupsPanel);
    expect(screen.getByText("Failed to list backups")).toBeTruthy();
  });

  it("renders empty state", () => {
    mockBackups.setState({
      backups: [],
      loading: false,
      totalSize: 0,
      error: null,
    });
    render(BackupsPanel);
    expect(screen.getByText("No backups yet.")).toBeTruthy();
  });
});
