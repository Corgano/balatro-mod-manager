import { render, screen, fireEvent } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import About from "./About.svelte";
import { openExternal } from "$lib/opener";

vi.mock("$lib/opener", () => ({
  openExternal: vi.fn(),
}));

describe("About view", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("opens the wiki page when wiki button is clicked", async () => {
    render(About);

    const wikiButton = screen.getByRole("button", { name: /visit wiki/i });
    await fireEvent.click(wikiButton);

    expect(openExternal).toHaveBeenCalledWith(
      "https://balatromods.miraheze.org/wiki/Main_Page",
    );
  });

  it("opens the BMM status page when status button is clicked", async () => {
    render(About);

    const statusButton = screen.getByRole("button", { name: /status page/i });
    await fireEvent.click(statusButton);

    expect(openExternal).toHaveBeenCalledWith(
      "https://status.dasguney.com/status/bmm",
    );
  });

  it("opens the Ko-fi page when support button is clicked", async () => {
    render(About);

    const kofiButton = screen.getByRole("button", {
      name: /support on ko-fi/i,
    });
    await fireEvent.click(kofiButton);

    expect(openExternal).toHaveBeenCalledWith(
      "https://ko-fi.com/skyline69/goal?g=0",
    );
  });
});
