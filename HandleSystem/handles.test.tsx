import { StrictMode, useEffect } from "react";
import { render, screen, cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { HandleSystem } from "./handles";
import { HandleProvider, useHandleOwner, useLockState } from "./react";
afterEach(cleanup);
describe("HandleSystem ownership", () => {
  it("keeps a resource locked until its final owner releases", () => {
    const system = new HandleSystem();
    const first = system.getLockHandle("shift", "submission");
    const second = system.getLockHandle("review", "submission");
    first.use();
    second.use();
    first.release();
    first.release();
    expect(system.isLocked("submission")).toBe(true);
    second.release();
    expect(system.isLocked("submission")).toBe(false);
  });
  it("cleans up only the disposed owner", () => {
    const system = new HandleSystem();
    system.getLockHandle("a", "modal").use();
    system.getLockHandle("b", "modal").use();
    system.releaseOwner("a");
    expect(system.isLocked("modal")).toBe(true);
    system.releaseOwner("b");
    expect(system.isLocked("modal")).toBe(false);
  });
  it("rejects use after release and duplicate activation", () => {
    const handle = new HandleSystem().getLockHandle("a", "form");
    handle.use();
    expect(() => handle.use()).toThrow();
    handle.release();
    expect(() => handle.use()).toThrow();
  });
  it("releases a failed operation and rejects overlapping submissions", async () => {
    const system = new HandleSystem();
    let finish!: () => void;
    const first = system.run(
      "a",
      "form",
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    await expect(system.run("b", "form", async () => 2)).rejects.toThrow();
    finish();
    await first;
    await expect(
      system.run("a", "form", async () => {
        throw new Error("network failure");
      }),
    ).rejects.toThrow();
    expect(system.isLocked("form")).toBe(false);
  });
  it("rolls back failed use actions", () => {
    const system = new HandleSystem();
    let active = false;
    const handle = system.getHandle("a", {
      onUse: () => {
        active = true;
        throw new Error("failed");
      },
      onRelease: () => {
        active = false;
      },
    });
    expect(() => handle.use()).toThrow();
    expect(active).toBe(false);
    expect(handle.state).toBe("released");
  });
  it("cleans component ownership in StrictMode without unlocking a second owner", () => {
    function Owner() {
      const { owner, system } = useHandleOwner();
      useEffect(() => {
        const handle = system.getLockHandle(owner, "modal");
        handle.use();
        return () => handle.release();
      }, [owner, system]);
      return null;
    }
    function Observer() {
      const locked = useLockState("modal");
      return <span>{locked ? "locked" : "unlocked"}</span>;
    }
    const tree = (first: boolean, second: boolean) => (
      <StrictMode>
        <HandleProvider>
          {first && <Owner key="a" />}
          {second && <Owner key="b" />}
          <Observer />
        </HandleProvider>
      </StrictMode>
    );
    const view = render(tree(true, true));
    expect(screen.getByText("locked")).toBeTruthy();
    view.rerender(tree(false, true));
    expect(screen.getByText("locked")).toBeTruthy();
    view.rerender(tree(false, false));
    expect(screen.getByText("unlocked")).toBeTruthy();
  });
});
