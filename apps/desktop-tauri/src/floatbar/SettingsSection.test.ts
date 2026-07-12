import { describe, expect, it } from "vitest";
import source from "./SettingsSection.tsx?raw";

describe("FloatBar settings", () => {
  it("renders one cost toggle", () => {
    expect(source.match(/floatBarShowCost: v/g)).toHaveLength(1);
  });
});
