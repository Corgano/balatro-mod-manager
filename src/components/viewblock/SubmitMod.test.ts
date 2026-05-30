import { render, screen, fireEvent } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import SubmitMod from "./SubmitMod.svelte";
import { openExternal } from "$lib/opener";

vi.mock("$lib/opener", () => ({
  openExternal: vi.fn(),
}));

describe("SubmitMod view", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("opens the submission helper when the button is clicked", async () => {
    render(SubmitMod);

    const button = screen.getByRole("button", {
      name: /open submission helper/i,
    });
    await fireEvent.click(button);

    expect(openExternal).toHaveBeenCalledWith(
      "https://bmi-helper.dasguney.com/",
    );
  });

  it("opens the mod index repo when the repo button is clicked", async () => {
    render(SubmitMod);

    const button = screen.getByRole("button", {
      name: /view mod index repo/i,
    });
    await fireEvent.click(button);

    expect(openExternal).toHaveBeenCalledWith(
      "https://github.com/skyline69/Balatro-Mod-Index",
    );
  });
});
