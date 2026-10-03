import { describe, it, expect } from "vitest";
import { bindingLabel, availableSiteCount } from "./presentation";
describe("actual state presentation", () => {
  it("distinguishes inherited bindings from overrides and absent dependencies", () => {
    expect(
      bindingLabel(
        { overrides: { php: "7.3.33" }, resolved: { php: "7.3.33" } },
        "php",
      ),
    ).toBe("7.3.33 · override");
    expect(
      bindingLabel({ overrides: {}, resolved: { php: "8.5.1" } }, "php"),
    ).toBe("8.5.1 · default");
    expect(bindingLabel({ overrides: {}, resolved: {} }, "mysql")).toBe(
      "Not configured",
    );
  });
  it("excludes removed folders from the live site count", () => {
    expect(availableSiteCount([{ present: true }, { present: false }])).toBe(1);
  });
});
