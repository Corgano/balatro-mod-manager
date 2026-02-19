import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { get } from "svelte/store";
import { beforeEach, describe, expect, it, vi } from "vitest";
import BulkActionBar from "./BulkActionBar.svelte";
import { modEnabledStore, selectedModsStore } from "../stores/modStore";

const invokeMock = vi.hoisted(() => vi.fn());
const addMessageMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("$lib/stores", () => ({
  addMessage: (...args: unknown[]) => addMessageMock(...args),
}));

if (!("animate" in Element.prototype)) {
  Object.defineProperty(Element.prototype, "animate", {
    value: () => ({
      cancel: () => {},
      play: () => {},
      pause: () => {},
      finished: Promise.resolve(),
    }),
    writable: true,
    configurable: true,
  });
}

describe("BulkActionBar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    selectedModsStore.set(new Set());
    modEnabledStore.set({});
    invokeMock.mockResolvedValue(null);
  });

  function renderBar(overrides?: {
    onBulkUninstall?: (ids: string[]) => void;
    onRefresh?: () => void;
  }) {
    return render(BulkActionBar, {
      props: {
        allSelectableIds: ["catalog-one", "/mods/local-one"],
        catalogModIds: ["catalog-one"],
        localModIds: ["/mods/local-one"],
        localModPaths: new Map([["/mods/local-one", "/mods/local-one"]]),
        localModNames: new Map([["/mods/local-one", "Local One"]]),
        onBulkUninstall: overrides?.onBulkUninstall ?? vi.fn(),
        onRefresh: overrides?.onRefresh ?? vi.fn(),
      },
    });
  }

  it("does not render when no mods are selected", () => {
    renderBar();
    expect(screen.queryByRole("button", { name: /uninstall/i })).toBeNull();
  });

  it("passes selected ids to the uninstall callback", async () => {
    selectedModsStore.set(new Set(["catalog-one", "/mods/local-one"]));
    const onBulkUninstall = vi.fn();
    renderBar({ onBulkUninstall });

    await fireEvent.click(screen.getByRole("button", { name: /uninstall/i }));

    expect(onBulkUninstall).toHaveBeenCalledWith([
      "catalog-one",
      "/mods/local-one",
    ]);
  });

  it("enables selected catalog and local mods, then refreshes", async () => {
    selectedModsStore.set(new Set(["catalog-one", "/mods/local-one"]));
    const onRefresh = vi.fn();
    renderBar({ onRefresh });

    await fireEvent.click(screen.getByRole("button", { name: /enable/i }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("toggle_mod_enabled", {
        modName: "catalog-one",
        enabled: true,
      });
      expect(invokeMock).toHaveBeenCalledWith("toggle_mod_enabled_by_path", {
        modPath: "/mods/local-one",
        enabled: true,
      });
      expect(onRefresh).toHaveBeenCalledTimes(1);
      expect(addMessageMock).toHaveBeenCalledWith("Enabled 2 mods", "success");
    });

    expect(get(modEnabledStore)).toMatchObject({
      "catalog-one": true,
      "Local One": true,
    });
    expect(get(selectedModsStore).size).toBe(0);
  });

  it("disables the disable button when all selected mods are already disabled", () => {
    selectedModsStore.set(new Set(["catalog-one"]));
    modEnabledStore.set({ "catalog-one": false });

    renderBar();

    const disableButton = screen.getByRole("button", { name: /disable/i });
    expect((disableButton as HTMLButtonElement).disabled).toBe(true);
  });
});
