import { act, render, screen } from "@testing-library/react";
import { Profiler } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api } from "../../api";
import { selectActiveTab, useStore } from "../../state/store";
import { connectedStore, table, text, value } from "../../test/fixtures";
import { objectDocument, orderAddress, orderGroups, source } from "../../test/objectFixtures";
import { StatusBar } from "../StatusBar";
import { ObjectPanel } from "./ObjectPanel";

const mocked = vi.mocked(api, true);

const COUNT = 5000;
const keyName = (n: number) => `zz_long_${String(n).padStart(4, "0")}`;

const longMeta = {
  ...orderGroups.meta,
  total: COUNT,
  capped: false,
  rows: Array.from({ length: COUNT }, (_, i) => {
    const id = String(1000 + i);
    return {
      key: [{ column: "id", value: id }],
      fields: [
        ["meta_key", value({ raw: text(keyName(i + 1)), source: source("wp_wc_orders_meta", "meta_key", id) })],
        ["meta_value", value({ raw: text(`value ${i + 1}`), source: source("wp_wc_orders_meta", "meta_value", id) })],
      ] as [string, ReturnType<typeof value>][],
    };
  }),
};

function Active({ onRender }: { onRender: () => void }) {
  const tab = useStore(selectActiveTab);
  if (tab?.kind !== "object") return null;
  return (
    <Profiler id="object" onRender={onRender}>
      <ObjectPanel tab={tab} />
    </Profiler>
  );
}

describe("object view with a long meta group", () => {
  const commits: string[] = [];
  const onRender = () => void commits.push("commit");

  beforeEach(() => {
    vi.clearAllMocks();
    commits.length = 0;
    connectedStore({ tabs: [], tables: [table("wp_wc_orders")] });
    mocked.document.mockResolvedValue(objectDocument({ groups: [orderGroups.own, longMeta] }));
  });

  async function open() {
    render(
      <>
        <Active onRender={onRender} />
        <StatusBar />
      </>,
    );
    act(() => void useStore.getState().openObject(orderAddress));
    await screen.findByRole("region", { name: "Object header" });
  }

  it("mounts only a bounded window of the meta keys", async () => {
    await open();
    expect(await screen.findByText(keyName(1))).toBeInTheDocument();
    expect(screen.queryByText(keyName(COUNT))).not.toBeInTheDocument();
    const shown = screen.queryAllByText(/^zz_long_\d{4}$/).length;
    expect(shown).toBeGreaterThan(0);
    expect(shown).toBeLessThan(200);
  });

  it("shows hovered provenance in the status bar without re-rendering the object view", async () => {
    await open();
    await screen.findByText(keyName(1));
    commits.length = 0;

    const sources = ["wp_wc_orders_meta.meta_value · id 1001", "wp_wc_orders_meta.meta_value · id 1002", "wp_wc_orders_meta.meta_key · id 1003"];
    for (const src of sources) {
      act(() => useStore.getState().setHover({ src, table: "wp_wc_orders_meta", linked: false }));
    }

    expect(commits).toHaveLength(0);
    expect(screen.getByText(sources[2])).toBeInTheDocument();
  });
});
