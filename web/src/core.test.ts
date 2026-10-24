import { describe, expect, it } from "vitest";
import { BasePhasedController } from "../../BasePhasedController/frontend/controller";
import { HandleSystem } from "../../HandleSystem/handles";
import { latin, milliliters, signedMilliliters } from "./localization";
describe("BasePhasedController", () => {
  const graph = {
    start: ["review"],
    review: ["start", "complete"],
    complete: [],
  } as const;
  it("enforces transitions, guards, previous history, cancellation, and reset", async () => {
    const controller = new BasePhasedController<
      "start" | "review" | "complete"
    >("start", graph, { guard: (_, to) => to !== "complete" });
    await controller.initialize();
    await expect(controller.transitionTo("complete")).rejects.toThrow();
    await controller.nextPhase();
    await expect(controller.transitionTo("complete")).rejects.toThrow();
    await controller.previousPhase();
    expect(controller.getCurrentPhase()).toBe("start");
    await controller.cancel();
    expect(controller.canTransition("review")).toBe(false);
    await controller.reset();
    expect(controller.canTransition("review")).toBe(true);
  });
  it("rejects a concurrent transition while a guard is awaiting", async () => {
    let finish!: (value: boolean) => void;
    const controller = new BasePhasedController<
      "start" | "review" | "complete"
    >("start", graph, {
      guard: () =>
        new Promise<boolean>((resolve) => {
          finish = resolve;
        }),
    });
    await controller.initialize();
    const first = controller.nextPhase();
    await expect(controller.nextPhase()).rejects.toThrow();
    finish(true);
    await first;
    expect(controller.getCurrentPhase()).toBe("review");
  });
  it("cleans failed phase handles before restoring the prior phase", async () => {
    const system = new HandleSystem();
    const controller = new BasePhasedController<
      "start" | "review" | "complete"
    >("start", graph, {
      enter: (phase) => {
        if (phase === "review") {
          system.getLockHandle("workflow", "form").use();
          throw new Error("entry failure");
        }
      },
      cleanup: () => system.releaseOwner("workflow"),
    });
    await controller.initialize();
    await expect(controller.nextPhase()).rejects.toThrow();
    expect(controller.getCurrentPhase()).toBe("start");
    expect(system.isLocked("form")).toBe(false);
  });
});
describe("fixed-point Persian input", () => {
  it("accepts Persian and Arabic digits without floating-point money math", () => {
    expect(latin("۱۲۳")).toBe("123");
    expect(latin("١٢٣")).toBe("123");
    expect(milliliters("۱۲٫۳۴۵")).toBe(12345);
  });
  it("rejects negative, imprecise, unsafe, and non-numeric measurements", () => {
    for (const value of ["-1", "1.0001", "abc", "9007199254740992"])
      expect(() => milliliters(value)).toThrow();
  });
  it("preserves signed inventory corrections with exact milliliter precision", () => {
    expect(signedMilliliters("-۱۲٫۳۴۵")).toBe(-12345);
    expect(signedMilliliters("۰٫۰۰۱")).toBe(1);
    expect(() => signedMilliliters("--1")).toThrow();
  });
});
