import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api, type Coverage, type ProjectRules, type RuleDraft } from "../../api";
import { installCommands, runCommand } from "../../commands";
import { referenceFormFor, type ProjectForm, type ReferenceForm } from "../../lib/ruleForms";
import { resultValueAt } from "../../lib/values";
import { selectActiveTab, useStore } from "../../state/store";
import { connectedStore, resultSet, table } from "../../test/fixtures";
import { browseOrderTables, incomingLink, objectDocument, orderAddress, orderGroups, paidViaSource } from "../../test/objectFixtures";
import { describeRuleTable, projectRules, referenceRow, woocommercePreset } from "../../test/projectFixtures";
import { ResultsGrid } from "../ResultsGrid";

const mocked = vi.mocked(api, true);

const tables = ["wp_wc_orders", "wp_wc_orders_meta", "wp_wc_order_addresses", "wp_wc_order_items", "wp_wc_order_coupon_lookup"].map((name) => table(name));

const coverage = (patch: Partial<Coverage> = {}): Coverage => ({ total: 0, resolved: 0, zeroOrNull: 0, sampleUnresolved: [], timedOut: false, ...patch });

/** After a project change the next `projectRules` reply is `rules`; `afterChange` may also prepare other replies. */
function respondToChangeWith(rules: ProjectRules, afterChange: () => void = () => {}) {
  mocked.projectUpdate.mockImplementation(async () => {
    mocked.projectRules.mockResolvedValue(rules);
    afterChange();
    return rules.project;
  });
}

async function openProjectPage(form?: ProjectForm) {
  render(<App />);
  await screen.findByRole("tree");
  act(() => useStore.getState().openProject(form ? { form } : undefined));
  await screen.findByRole("region", { name: "Presets and prefix" });
}

async function openOrderObject() {
  render(<App />);
  await screen.findByRole("tree");
  act(() => useStore.getState().openObject(orderAddress));
  await screen.findByRole("region", { name: "Object header" });
}

const ORDER_ITEMS_ROW = "wp_wc_order_items.order_id → wp_wc_orders.id";

describe("ProjectPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [], tables });
    installCommands();
    mocked.document.mockResolvedValue(objectDocument());
    mocked.browseTable.mockImplementation(browseOrderTables);
    mocked.describeTable.mockImplementation(describeRuleTable);
    mocked.projectRules.mockResolvedValue(projectRules());
    mocked.projectUpdate.mockResolvedValue(projectRules().project);
    mocked.coveragePreview.mockResolvedValue(coverage());
    mocked.referenceCoverage.mockResolvedValue(coverage());
    mocked.chooseSavePath.mockResolvedValue(null);
    mocked.rulesInfo.mockResolvedValue({ prefix: "wp_", networkPrefix: "wp_", suggestedPrefix: null, appliedPresets: ["woocommerce"], availablePresets: ["woocommerce"], entities: [], tableEntities: {} });
  });

  describe("opening the page", () => {
    it.each([
      ["an address with ?project", () => useStore.getState().openAddress("databee://shop/db/?project")],
      ["the project.open command", () => runCommand("project.open")],
    ])("opens a Project tab from %s", async (_how, open) => {
      render(<App />);
      await screen.findByRole("tree");
      await act(async () => {
        await open();
      });
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "project", database: "db" });
      expect(await screen.findByRole("region", { name: "Presets and prefix" })).toBeInTheDocument();
    });

    it("opens a Project tab from the status bar's presets button, which names the applied presets", async () => {
      render(<App />);
      await userEvent.click(await screen.findByRole("button", { name: "presets woocommerce" }));
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "project", database: "db" });
      expect(await screen.findByRole("region", { name: "References" })).toBeInTheDocument();
    });

    it("reuses the open Project tab", async () => {
      await openProjectPage();
      await act(async () => {
        runCommand("project.open");
      });
      expect(useStore.getState().tabs.filter((t) => t.kind === "project")).toHaveLength(1);
    });

    it("shows the error when the rules cannot be loaded", async () => {
      mocked.projectRules.mockRejectedValue({ kind: "invalid_input", message: "No rules for this database", code: null });
      render(<App />);
      await screen.findByRole("tree");
      act(() => useStore.getState().openProject());
      expect(await screen.findByText("No rules for this database")).toBeInTheDocument();
    });
  });

  describe("presets and prefix", () => {
    const presets = [woocommercePreset, { name: "yoast", markerTables: [], applied: true }, { name: "acf", markerTables: ["wp_acf"], applied: false }];

    beforeEach(() => mocked.projectRules.mockResolvedValue(projectRules({ presets })));

    it.each([
      ["off", "yoast", ["woocommerce"]],
      ["on", "acf", ["woocommerce", "yoast", "acf"]],
    ])("sends the explicit list when preset %s is switched", async (_state, name, expected) => {
      await openProjectPage();
      await userEvent.click(screen.getByRole("switch", { name: `Preset ${name}` }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "setPresets", presets: expected });
    });

    it("shows each preset's marker tables", async () => {
      await openProjectPage();
      expect(screen.getByText("marker wp_wc_orders")).toBeInTheDocument();
      expect(screen.getByText("no marker tables")).toBeInTheDocument();
    });

    it("says presets are automatic until some are chosen by hand", async () => {
      await openProjectPage();
      expect(screen.getByText("auto: every preset whose marker tables exist")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "back to auto" })).not.toBeInTheDocument();
    });

    it("goes back to auto from a hand-picked list", async () => {
      const chosen = projectRules({ presets });
      mocked.projectRules.mockResolvedValue({ ...chosen, project: { ...chosen.project, presets: ["woocommerce"] } });
      await openProjectPage();
      expect(screen.getByText("chosen by hand")).toBeInTheDocument();
      await userEvent.click(screen.getByRole("button", { name: "back to auto" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "setPresets", presets: null });
    });
  });

  describe("export", () => {
    it("asks for a path named after the connection and database, then exports there", async () => {
      mocked.chooseSavePath.mockResolvedValue("/tmp/shop-db.toml");
      await openProjectPage();
      await userEvent.click(screen.getByRole("button", { name: "Export as preset…" }));
      expect(mocked.chooseSavePath).toHaveBeenCalledWith("shop-db.toml", "toml");
      expect(await screen.findByText("Exported the project to /tmp/shop-db.toml")).toBeInTheDocument();
      expect(mocked.projectExportTo).toHaveBeenCalledWith("s1", "db", "/tmp/shop-db.toml");
    });

    it("exports nothing when the save dialog is cancelled", async () => {
      await openProjectPage();
      await userEvent.click(screen.getByRole("button", { name: "Export as preset…" }));
      await waitFor(() => expect(mocked.chooseSavePath).toHaveBeenCalled());
      expect(mocked.projectExportTo).not.toHaveBeenCalled();
    });
  });

  describe("adding a reference from a meta value", () => {
    const draft: RuleDraft = { source: "wc_orders_meta.meta_value", meta: { table: "wc_orders_meta", keyColumn: "meta_key", key: "_ppcp_paypal_order_id" } };
    const paypalGroup = { ...orderGroups.refunds, id: "paypal", via: incomingLink("wp_wc_orders_meta", "meta_value", "PayPal orders") };

    beforeEach(() => mocked.ruleDraft.mockResolvedValue(draft));

    it("fills the form from the value, saves the rule and refreshes the open object", async () => {
      await openOrderObject();
      await userEvent.click(screen.getByText("bacs"));
      await userEvent.keyboard("r");

      const form = await screen.findByRole("region", { name: "new reference" });
      expect(mocked.ruleDraft).toHaveBeenCalledWith("s1", "db", paidViaSource);
      expect(within(form).getByRole("combobox", { name: "Source table" })).toHaveValue("wp_wc_orders_meta");
      expect(within(form).getByRole("combobox", { name: "When column" })).toHaveValue("meta_key");
      expect(within(form).getByRole("textbox", { name: "When value" })).toHaveValue("_ppcp_paypal_order_id");

      await userEvent.selectOptions(within(form).getByRole("combobox", { name: "Target table" }), "wp_wc_orders");
      await waitFor(() => expect(within(form).getByRole("combobox", { name: "Target column" })).toHaveValue("id"));

      respondToChangeWith(projectRules(), () => mocked.document.mockResolvedValue(objectDocument({ groups: [...Object.values(orderGroups), paypalGroup] })));
      await userEvent.click(within(form).getByRole("button", { name: "Save to project" }));

      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", {
        op: "addReference",
        reference: expect.objectContaining({ from: "wc_orders_meta.meta_value", when: { meta_key: "_ppcp_paypal_order_id" }, to: "wc_orders.id" }),
      });
      expect(await screen.findByText("PayPal orders · meta_value → this row")).toBeInTheDocument();
      expect(screen.queryByRole("region", { name: "new reference" })).not.toBeInTheDocument();
    });

    it("opens the same form from the Add reference… menu item", async () => {
      await openOrderObject();
      fireEvent.contextMenu(screen.getByText("bacs"));
      await userEvent.click(screen.getByRole("menuitem", { name: "Add reference…" }));
      const form = await screen.findByRole("region", { name: "new reference" });
      expect(within(form).getByRole("textbox", { name: "When value" })).toHaveValue("_ppcp_paypal_order_id");
    });

    it("saves nothing and asks for a target while none is picked", async () => {
      await openOrderObject();
      await userEvent.click(screen.getByText("bacs"));
      await userEvent.keyboard("r");
      const form = await screen.findByRole("region", { name: "new reference" });
      await userEvent.click(within(form).getByRole("button", { name: "Save to project" }));
      expect(await within(form).findByRole("alert")).toHaveTextContent("pick a target table and column");
      expect(mocked.projectUpdate).not.toHaveBeenCalled();
    });
  });

  describe("the reference form", () => {
    const form = (): ReferenceForm => ({
      ...referenceFormFor(paidViaSource, { source: "wc_orders_meta.meta_value", meta: null }),
      target: { table: "wp_wc_orders", column: "id" },
    });

    it("previews the coverage of the unsaved rule and warns when nothing resolves", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      try {
        mocked.coveragePreview.mockResolvedValue(coverage({ total: 1, sampleUnresolved: ["8YB…"] }));
        await openProjectPage(form());
        await act(async () => {
          await vi.advanceTimersByTimeAsync(400);
        });
        expect(await screen.findByText("0 of 1 resolve")).toBeInTheDocument();
        expect(screen.getByRole("alert")).toHaveTextContent("no source value matches a target; the rule is probably wrong · unresolved: 8YB…");
        expect(mocked.coveragePreview).toHaveBeenLastCalledWith("s1", "db", expect.objectContaining({ from: "wc_orders_meta.meta_value", to: "wc_orders.id" }), null);
      } finally {
        vi.useRealTimers();
      }
    });

    it("shows the Rust error under the field it names", async () => {
      mocked.projectUpdate.mockRejectedValue({ kind: "invalid_input", message: "`reference.to`: `wp_users` has no column `nope`", code: null });
      await openProjectPage({ ...form(), target: { table: "wp_users", column: "nope" } });
      const region = await screen.findByRole("region", { name: "new reference" });
      await userEvent.click(within(region).getByRole("button", { name: "Save to project" }));
      const alert = await within(region).findByRole("alert");
      expect(alert).toHaveTextContent("`wp_users` has no column `nope`");
      expect(alert.parentElement).toContainElement(within(region).getByRole("combobox", { name: "Target table" }));
      expect(screen.getByRole("region", { name: "new reference" })).toBeInTheDocument();
    });

    it("closes without saving on Cancel", async () => {
      await openProjectPage(form());
      await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
      expect(screen.queryByRole("region", { name: "new reference" })).not.toBeInTheDocument();
      expect(mocked.projectUpdate).not.toHaveBeenCalled();
    });
  });

  describe("references", () => {
    const rowIn = (name = ORDER_ITEMS_ROW) => screen.findByRole("row", { name });

    it("disables a preset reference and shows it struck through with an enable button", async () => {
      mocked.projectRules.mockResolvedValue(projectRules({ references: [referenceRow()] }));
      respondToChangeWith(projectRules({ references: [referenceRow({ disabled: true })] }));
      await openProjectPage();
      await userEvent.click(within(await rowIn()).getByRole("button", { name: "disable" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "disable", id: "x" });
      await waitFor(() => expect(within(screen.getByRole("row", { name: ORDER_ITEMS_ROW })).getByRole("button", { name: "enable" })).toBeInTheDocument());
      const row = screen.getByRole("row", { name: ORDER_ITEMS_ROW });
      expect(row).toHaveClass("off");
      expect(within(row).getByText("· disabled")).toBeInTheDocument();
    });

    it("enables a disabled reference", async () => {
      mocked.projectRules.mockResolvedValue(projectRules({ references: [referenceRow({ disabled: true })] }));
      await openProjectPage();
      await userEvent.click(within(await rowIn()).getByRole("button", { name: "enable" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "enable", id: "x" });
    });

    it("promotes an inferred reference to a project rule, which can then be edited or removed", async () => {
      const inferred = referenceRow({ origin: { kind: "inferred", indexed: true }, id: "inferred-1" });
      mocked.projectRules.mockResolvedValue(projectRules({ references: [inferred] }));
      respondToChangeWith(projectRules({ references: [referenceRow({ origin: { kind: "project" }, id: "p1" })] }));
      await openProjectPage();
      expect(within(await rowIn()).getByText(/inferred/)).toBeInTheDocument();
      await userEvent.click(within(await rowIn()).getByRole("button", { name: "promote" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "addReference", reference: inferred.rule });
      await waitFor(() => expect(within(screen.getByRole("row", { name: ORDER_ITEMS_ROW })).getByRole("button", { name: "remove" })).toBeInTheDocument());
      const row = screen.getByRole("row", { name: ORDER_ITEMS_ROW });
      expect(within(row).getByText("project")).toBeInTheDocument();
      expect(within(row).getByRole("button", { name: "edit" })).toBeInTheDocument();
    });

    it("removes a project reference by id", async () => {
      mocked.projectRules.mockResolvedValue(projectRules({ references: [referenceRow({ origin: { kind: "project" }, id: "p1" })] }));
      await openProjectPage();
      await userEvent.click(within(await rowIn()).getByRole("button", { name: "remove" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "removeReference", id: "p1" });
    });

    it("opens a project reference in the form to edit it", async () => {
      mocked.projectRules.mockResolvedValue(projectRules({ references: [referenceRow({ origin: { kind: "project" }, id: "p1" })] }));
      await openProjectPage();
      await userEvent.click(within(await rowIn()).getByRole("button", { name: "edit" }));
      const form = await screen.findByRole("region", { name: "edit reference" });
      expect(within(form).getByRole("combobox", { name: "Source table" })).toHaveValue("wp_wc_order_items");
      await userEvent.click(within(form).getByRole("button", { name: "Save to project" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "updateReference", id: "p1", reference: expect.objectContaining({ from: "wc_order_items.order_id", to: "wc_orders.id" }) });
    });

    describe("coverage", () => {
      beforeEach(() => mocked.projectRules.mockResolvedValue(projectRules({ references: [referenceRow()] })));

      it("says a row is not checked until it is", async () => {
        await openProjectPage();
        expect(within(await rowIn()).getByText("not checked")).toBeInTheDocument();
      });

      it("shows resolved of total and the 0-or-NULL count of a checked row", async () => {
        mocked.referenceCoverage.mockResolvedValue(coverage({ total: 7, resolved: 6, zeroOrNull: 1 }));
        await openProjectPage();
        await userEvent.click(within(await rowIn()).getByRole("button", { name: "check" }));
        const row = await rowIn();
        expect(await within(row).findByText("6 of 7")).toBeInTheDocument();
        expect(within(row).getByText("· 1 is 0 or NULL")).toBeInTheDocument();
        expect(mocked.referenceCoverage).toHaveBeenCalledWith("s1", "db", referenceRow().reference);
      });

      it("says so when the count timed out", async () => {
        mocked.referenceCoverage.mockResolvedValue(coverage({ timedOut: true }));
        await openProjectPage();
        await userEvent.click(within(await rowIn()).getByRole("button", { name: "check" }));
        expect(await within(await rowIn()).findByText("timed out")).toBeInTheDocument();
      });

      it("shows the error of a failed check in the row", async () => {
        mocked.referenceCoverage.mockRejectedValue({ kind: "invalid_input", message: "Table is gone", code: null });
        await openProjectPage();
        await userEvent.click(within(await rowIn()).getByRole("button", { name: "check" }));
        expect(await within(await rowIn()).findByText("Table is gone")).toBeInTheDocument();
      });

      it("checks every applied row with Check coverage for all and leaves unapplied rows alone", async () => {
        const meta = referenceRow({ id: "y", from: { table: "wp_wc_orders_meta", columns: ["order_id"] }, reference: { ...referenceRow().reference!, from: { table: "wp_wc_orders_meta", columns: ["order_id"] } } });
        const foreignKey = referenceRow({ origin: { kind: "foreignKey", constraint: "fk_user" }, id: null, from: { table: "wp_users", columns: ["ID"] }, reference: null, rule: null });
        mocked.projectRules.mockResolvedValue(projectRules({ references: [referenceRow(), meta, foreignKey] }));
        mocked.referenceCoverage.mockImplementation(async (_s, _d, reference) => (reference.from.table === "wp_wc_order_items" ? coverage({ total: 4, resolved: 3 }) : coverage({ total: 2, resolved: 1, zeroOrNull: 1 })));
        await openProjectPage();
        await screen.findByRole("row", { name: ORDER_ITEMS_ROW });
        await userEvent.click(screen.getByRole("button", { name: "Check coverage for all" }));
        expect(await screen.findByText("3 of 4")).toBeInTheDocument();
        expect(await screen.findByText("1 of 2")).toBeInTheDocument();
        const unapplied = screen.getByRole("row", { name: "wp_users.ID → wp_wc_orders.id" });
        expect(within(unapplied).getByText("not applied")).toBeInTheDocument();
        expect(within(unapplied).queryByRole("button", { name: "check" })).not.toBeInTheDocument();
      });
    });
  });

  describe("adding a value style", () => {
    const styleRuleStored = { columns: ["wc_orders.status"], meta_keys: [], in: [], match: "=" as const, value: "wc-failed", color: "red" as const, enabled: true };

    beforeEach(() => mocked.ruleDraft.mockResolvedValue({ source: "wc_orders.status", meta: null }));

    it("takes the value's column, saves the chosen match, text and color, and lists the style", async () => {
      await openOrderObject();
      fireEvent.contextMenu(screen.getByText("wc-refunded"));
      await userEvent.click(screen.getByRole("menuitem", { name: "Style…" }));

      const form = await screen.findByRole("region", { name: "new value style" });
      expect(within(form).getByRole("textbox", { name: "columns" })).toHaveValue("wc_orders.status");
      const value = within(form).getByRole("textbox", { name: "Value" });
      expect(value).toHaveValue("wc-refunded");
      await userEvent.clear(value);
      await userEvent.type(value, "wc-failed");
      expect(within(form).getByRole("button", { name: "red" })).toHaveAttribute("aria-pressed", "true");
      expect(within(form).getByRole("button", { name: "blue" })).toHaveAttribute("aria-pressed", "false");
      await userEvent.click(within(form).getByRole("button", { name: "red" }));

      respondToChangeWith(projectRules({ styles: [{ origin: { kind: "project" }, index: 0, rule: styleRuleStored }] }));
      await userEvent.click(within(form).getByRole("button", { name: "Save to project" }));

      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "addStyle", rule: expect.objectContaining({ columns: ["wc_orders.status"], match: "=", value: "wc-failed", color: "red", enabled: true }) });
      const styles = screen.getByRole("region", { name: "Value styles" });
      expect(await within(styles).findByRole("row", { name: "wc_orders.status = 'wc-failed'" })).toBeInTheDocument();
    });

    it("moves the pressed state when another color is picked", async () => {
      await openOrderObject();
      fireEvent.contextMenu(screen.getByText("wc-refunded"));
      await userEvent.click(screen.getByRole("menuitem", { name: "Style…" }));
      const form = await screen.findByRole("region", { name: "new value style" });
      await userEvent.click(within(form).getByRole("button", { name: "green" }));
      expect(within(form).getByRole("button", { name: "green" })).toHaveAttribute("aria-pressed", "true");
      expect(within(form).getByRole("button", { name: "red" })).toHaveAttribute("aria-pressed", "false");
    });

    it("lists a style and removes it by index", async () => {
      mocked.projectRules.mockResolvedValue(projectRules({ styles: [{ origin: { kind: "project" }, index: 3, rule: styleRuleStored }] }));
      await openProjectPage();
      const row = await screen.findByRole("row", { name: "wc_orders.status = 'wc-failed'" });
      await userEvent.click(within(row).getByRole("button", { name: "remove" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "removeStyle", index: 3 });
    });

    it("copies a preset's style that ships off into the project when it is switched on", async () => {
      const off = { ...styleRuleStored, enabled: false };
      mocked.projectRules.mockResolvedValue(projectRules({ styles: [{ origin: { kind: "preset", preset: "woocommerce" }, index: null, rule: off }] }));
      await openProjectPage();
      await userEvent.click(await screen.findByRole("button", { name: "switch on" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "addStyle", rule: { ...off, enabled: true } });
    });
  });

  describe("value styles in grids", () => {
    it("draws a cell whose value has a style in that scheme color", () => {
      const rs = resultSet({ styles: [{ row: 0, col: 1, style: "red" }] });
      render(<ResultsGrid columns={rs.columns} rows={rs.rows} source={{ sessionId: "s1", resultId: 1 }} valueAt={(row, col) => resultValueAt(rs, row, col)} />);
      expect(screen.getByText("Hello world").closest("[style]")?.getAttribute("style")).toContain("var(--style-red)");
    });
  });

  describe("kinds", () => {
    it("saves a kind for the meta key of the focused value", async () => {
      mocked.ruleDraft.mockResolvedValue({ source: "wc_orders_meta.meta_value", meta: { table: "wc_orders_meta", keyColumn: "meta_key", key: "_paid_via" } });
      await openOrderObject();
      fireEvent.contextMenu(screen.getByText("bacs"));
      await userEvent.click(screen.getByRole("menuitem", { name: "Kind…" }));
      const form = await screen.findByRole("region", { name: "new kind" });
      expect(within(form).getByRole("textbox", { name: "meta keys" })).toHaveValue("_paid_via");
      expect(within(form).getByRole("textbox", { name: "in meta tables" })).toHaveValue("wc_orders_meta");
      await userEvent.selectOptions(within(form).getByRole("combobox", { name: "Kind" }), "secret");
      await userEvent.click(within(form).getByRole("button", { name: "Save to project" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "addKind", rule: { columns: [], meta_keys: ["_paid_via"], in: ["wc_orders_meta"], secret: true } });
    });
  });

  describe("prefix", () => {
    it("sends the typed prefix when the field is left", async () => {
      await openProjectPage();
      await userEvent.type(screen.getByRole("textbox", { name: "prefix" }), "wc_");
      await userEvent.tab();
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "setPrefix", prefix: "wc_" });
    });

    it("shows the error Rust returns in place of the suggestion", async () => {
      mocked.projectUpdate.mockRejectedValue({ kind: "invalid_input", message: "A prefix must end in an underscore", code: null });
      await openProjectPage();
      expect(screen.getByText("suggested: wp_")).toBeInTheDocument();
      await userEvent.type(screen.getByRole("textbox", { name: "prefix" }), "wc{Enter}");
      expect(await screen.findByText("A prefix must end in an underscore")).toBeInTheDocument();
      expect(screen.queryByText("suggested: wp_")).not.toBeInTheDocument();
    });

    it("says there is no suggestion when Rust has none", async () => {
      mocked.projectRules.mockResolvedValue(projectRules({ suggestedPrefix: null }));
      await openProjectPage();
      expect(screen.getByText("no suggestion")).toBeInTheDocument();
    });
  });

  describe("editing and removing project rules", () => {
    const kindRow = { origin: { kind: "project" } as const, index: 2, rule: { columns: ["wc_orders.status"], secret: true } };
    const styleRow = { origin: { kind: "project" } as const, index: 3, rule: { columns: ["wc_orders.status"], match: "=" as const, value: "wc-failed", color: "red" as const, enabled: true } };

    beforeEach(() => mocked.projectRules.mockResolvedValue(projectRules({ kinds: [kindRow], styles: [styleRow] })));

    it("updates a kind by index with the edited rule", async () => {
      await openProjectPage();
      await userEvent.click(within(await screen.findByRole("row", { name: "wc_orders.status secret" })).getByRole("button", { name: "edit" }));
      const form = await screen.findByRole("region", { name: "edit kind" });
      await userEvent.selectOptions(within(form).getByRole("combobox", { name: "Kind" }), "number");
      await userEvent.click(within(form).getByRole("button", { name: "Save to project" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "updateKind", index: 2, rule: expect.objectContaining({ columns: ["wc_orders.status"], number: true }) });
    });

    it("updates a style by index with the edited rule", async () => {
      await openProjectPage();
      await userEvent.click(within(await screen.findByRole("row", { name: "wc_orders.status = 'wc-failed'" })).getByRole("button", { name: "edit" }));
      const form = await screen.findByRole("region", { name: "edit value style" });
      const value = within(form).getByRole("textbox", { name: "Value" });
      await userEvent.clear(value);
      await userEvent.type(value, "wc-cancelled");
      await userEvent.click(within(form).getByRole("button", { name: "Save to project" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "updateStyle", index: 3, rule: expect.objectContaining({ columns: ["wc_orders.status"], value: "wc-cancelled", color: "red" }) });
    });

    it("removes a kind by index", async () => {
      await openProjectPage();
      await userEvent.click(within(await screen.findByRole("row", { name: "wc_orders.status secret" })).getByRole("button", { name: "remove" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "removeKind", index: 2 });
    });
  });

  describe("long rule lists", () => {
    const pad = (n: number) => String(n).padStart(4, "0");

    it("renders only a window of 500 references", async () => {
      const references = Array.from({ length: 500 }, (_, i) =>
        referenceRow({ id: `r${i}`, from: { table: "wp_wc_order_items", columns: [`ref_${pad(i + 1)}`] } }),
      );
      mocked.projectRules.mockResolvedValue(projectRules({ references }));
      await openProjectPage();
      const section = within(await screen.findByRole("region", { name: "References" }));
      expect(await section.findByText("wp_wc_order_items.ref_0001")).toBeInTheDocument();
      expect(section.queryByText("wp_wc_order_items.ref_0500")).not.toBeInTheDocument();
      expect(section.getAllByRole("row").length).toBeLessThan(200);
    });

    it("renders only a window of 500 kinds", async () => {
      const kinds = Array.from({ length: 500 }, (_, i) => ({ origin: { kind: "project" } as const, index: i, rule: { columns: [`kind_${pad(i + 1)}`], secret: true } }));
      mocked.projectRules.mockResolvedValue(projectRules({ kinds }));
      await openProjectPage();
      const section = within(await screen.findByRole("region", { name: "Kinds" }));
      expect(await section.findByText("kind_0001")).toBeInTheDocument();
      expect(section.queryByText("kind_0500")).not.toBeInTheDocument();
      expect(section.getAllByRole("row").length).toBeLessThan(200);
    });
  });
});
