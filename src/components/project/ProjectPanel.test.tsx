import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import App from "../../App";
import { api, type Coverage, type KindRow, type ProjectRules, type ReferenceRow, type RuleDraft, type StyleRow, type TableRules } from "../../api";
import { installCommands, runCommand } from "../../commands";
import { referenceFormFor, type ProjectForm, type ReferenceForm } from "../../lib/ruleForms";
import { resultValueAt } from "../../lib/values";
import type { ProjectSection } from "../../state/slices/tabs";
import { selectActiveTab, useStore, type TableTab } from "../../state/store";
import { connectedStore, readoutOf, resultSet, table } from "../../test/fixtures";
import { browseOrderTables, objectDocument, orderAddress, paidViaSource } from "../../test/objectFixtures";
import { changeLine, describeRuleTable, orderItemsRule, projectRules, referenceRow, tableRules, woocommercePreset } from "../../test/projectFixtures";
import { ResultsGrid } from "../ResultsGrid";

const mocked = vi.mocked(api, true);

const tables = ["wp_wc_orders", "wp_wc_orders_meta", "wp_wc_order_addresses", "wp_wc_order_items", "wp_wc_order_coupon_lookup"].map((name) => table(name));

const coverage = (patch: Partial<Coverage> = {}): Coverage => ({ total: 0, resolved: 0, zeroOrNull: 0, sampleUnresolved: [], timedOut: false, ...patch });

function respondToChangeWith(rules: ProjectRules) {
  mocked.projectUpdate.mockImplementation(async () => {
    mocked.projectRules.mockResolvedValue(rules);
    return rules.project;
  });
}

async function openProjectPage(form?: ProjectForm, section: ProjectSection = "rules") {
  const view = render(<App />);
  await screen.findByRole("tree");
  act(() => {
    useStore.getState().openProject(form ? { form } : undefined);
    useStore.getState().setProjectSection(selectActiveTab(useStore.getState())!.id, section);
  });
  await screen.findByRole("region", { name: "Presets and prefix" });
  return view;
}

async function openOrderObject() {
  render(<App />);
  await screen.findByRole("tree");
  act(() => useStore.getState().openObject(orderAddress));
  await screen.findByRole("region", { name: "Object header" });
}

const ORDER_ITEMS_ROW = "wp_wc_order_items.order_id → wp_wc_orders.id";
const FK_ROW = "wp_users.ID → wp_wc_orders.id";
const STYLE_ID = "style:wc_orders.status=wc-failed";
const STYLE_NAME = "wc_orders.status = 'wc-failed'";
const KIND_NAME = "wc_orders.status secret";

const itemsGroup = (patch: Partial<TableRules> = {}) => tableRules("wp_wc_order_items", patch);
const rulesWith = (patch: Partial<TableRules>) => projectRules({ groups: [itemsGroup(patch)] });
const tableRegion = (name = "wp_wc_order_items") => screen.findByRole("region", { name });

function itemsReference(column: string, patch: Partial<ReferenceRow> = {}): ReferenceRow {
  const base = referenceRow();
  const from = { table: "wp_wc_order_items", columns: [column] };
  return { ...base, id: `r-${column}`, from, reference: { ...base.reference!, from }, ...patch };
}

const inferredRow = (patch: Partial<ReferenceRow> = {}) =>
  referenceRow({ origin: { kind: "inferred", indexed: true }, id: "inferred-1", rule: { ...orderItemsRule, id: "" }, ...patch });

const foreignKeyRow = () => referenceRow({ origin: { kind: "foreignKey", constraint: "fk_user" }, id: null, from: { table: "wp_users", columns: ["ID"] }, reference: null, rule: null });

const styleRule = { columns: ["wc_orders.status"], meta_keys: [], in: [], match: "=" as const, value: "wc-failed", color: "red" as const, enabled: true };

const styleRow = (patch: Partial<StyleRow> = {}): StyleRow => ({ origin: { kind: "project" }, id: STYLE_ID, rule: styleRule, disabled: false, edited: false, ...patch });

const presetStyle = (patch: Partial<StyleRow> = {}) => styleRow({ origin: { kind: "preset", preset: "woocommerce" }, ...patch });

const kindRow = (patch: Partial<KindRow> = {}): KindRow => ({ origin: { kind: "project" }, index: 2, rule: { columns: ["wc_orders.status"], secret: true }, ...patch });

function deferredCoverage() {
  const pending: Array<(c: Coverage) => void> = [];
  mocked.referenceCoverage.mockImplementation(() => new Promise<Coverage>((resolve) => pending.push(resolve)));
  return pending;
}

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
    mocked.readout.mockResolvedValue(readoutOf());
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
      expect(await screen.findByRole("region", { name: "Presets and prefix" })).toBeInTheDocument();
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

    it("says no table has rules when there are no groups", async () => {
      await openProjectPage();
      expect(screen.getByText(/^no rules:/)).toBeInTheDocument();
    });
  });

  describe("header", () => {
    const presets = [woocommercePreset, { name: "yoast", markerTables: [], applied: true }, { name: "acf", markerTables: ["wp_acf"], applied: false }];

    beforeEach(() => mocked.projectRules.mockResolvedValue(projectRules({ presets })));

    it("holds the prefix field and a switch per preset in one region", async () => {
      await openProjectPage();
      const header = screen.getByRole("region", { name: "Presets and prefix" });
      expect(within(header).getByRole("textbox", { name: "prefix" })).toBeInTheDocument();
      expect(within(header).getByRole("switch", { name: "Preset woocommerce" })).toBeChecked();
      expect(within(header).getByRole("switch", { name: "Preset yoast" })).toBeChecked();
      expect(within(header).getByRole("switch", { name: "Preset acf" })).not.toBeChecked();
    });

    it.each([
      ["off", "yoast", ["woocommerce"]],
      ["on", "acf", ["woocommerce", "yoast", "acf"]],
    ])("sends the explicit list when preset %s is switched", async (_state, name, expected) => {
      await openProjectPage();
      await userEvent.click(screen.getByRole("switch", { name: `Preset ${name}` }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "setPresets", presets: expected });
    });

    it.each([
      ["woocommerce", "marker tables wp_wc_orders"],
      ["yoast", "no marker tables"],
    ])("names the marker tables of preset %s in its tooltip", async (name, title) => {
      await openProjectPage();
      expect(screen.getByRole("switch", { name: `Preset ${name}` }).closest("label")).toHaveAttribute("title", title);
    });

    it("says auto while presets are automatic", async () => {
      await openProjectPage();
      const header = screen.getByRole("region", { name: "Presets and prefix" });
      expect(within(header).getByText("auto")).toBeInTheDocument();
      expect(within(header).queryByRole("button", { name: "back to auto" })).not.toBeInTheDocument();
    });

    it("goes back to auto from a hand-picked list", async () => {
      const chosen = projectRules({ presets });
      mocked.projectRules.mockResolvedValue({ ...chosen, project: { ...chosen.project, presets: ["woocommerce"] } });
      await openProjectPage();
      const header = screen.getByRole("region", { name: "Presets and prefix" });
      expect(within(header).queryByText("auto")).not.toBeInTheDocument();
      await userEvent.click(within(header).getByRole("button", { name: "back to auto" }));
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

  describe("overview", () => {
    const openOverview = () => openProjectPage(undefined, "overview");
    const activeTable = () => selectActiveTab(useStore.getState()) as TableTab;
    const withReference = (patch: Partial<ProjectRules> = {}) => projectRules({ groups: [itemsGroup({ references: [referenceRow()] })], ...patch });
    const problemRow = () => screen.findByRole("row", { name: ORDER_ITEMS_ROW });

    it("opens on the overview with changes, problems and a way to all rules", async () => {
      render(<App />);
      await screen.findByRole("tree");
      act(() => useStore.getState().openProject());
      expect(await screen.findByRole("region", { name: "Your changes" })).toBeInTheDocument();
      expect(screen.getByRole("region", { name: "Problems" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "All rules…" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Overview" })).toHaveAttribute("aria-pressed", "true");
      expect(screen.queryByRole("searchbox", { name: "Filter tables" })).not.toBeInTheDocument();
    });

    it("says so when nothing differs from the presets and nothing is wrong", async () => {
      await openOverview();
      expect(screen.getByText("No changes — this database uses the presets as shipped.")).toBeInTheDocument();
      expect(await screen.findByText("No problems found.")).toBeInTheDocument();
    });

    describe("your changes", () => {
      it("lists a change as a sentence with its origin, id and what the preset shipped", async () => {
        mocked.projectRules.mockResolvedValue(projectRules({ changes: [changeLine({ was: "wp_wc_order_items.order_id → wp_wc_orders.order_id" })] }));
        await openOverview();
        const row = await screen.findByRole("row", { name: `edited reference ${ORDER_ITEMS_ROW}` });
        expect(row).toHaveTextContent("edited reference · preset woocommerce · x");
        expect(row).toHaveTextContent("· was wp_wc_order_items.order_id → wp_wc_orders.order_id");
        expect(screen.queryByText("No changes — this database uses the presets as shipped.")).not.toBeInTheDocument();
      });

      it("marks an added rule that is stored off", async () => {
        mocked.projectRules.mockResolvedValue(projectRules({ changes: [changeLine({ action: "added", origin: null, off: true })] }));
        await openOverview();
        expect(await screen.findByRole("row", { name: `added reference ${ORDER_ITEMS_ROW}` })).toHaveTextContent("· off");
      });

      it("says suggested for a prefix and gives a setting no link to a table", async () => {
        mocked.projectRules.mockResolvedValue(
          projectRules({ changes: [changeLine({ action: "set", rule: "prefix", origin: null, id: null, text: "wc_", was: "wp_", table: null, undo: { op: "setPrefix", prefix: null }, edit: null })] }),
        );
        await openOverview();
        const row = await screen.findByRole("row", { name: "set prefix wc_" });
        expect(row).toHaveTextContent("· suggested wp_");
        expect(within(row).queryByRole("button", { name: "wc_" })).not.toBeInTheDocument();
      });

      it("says auto for presets", async () => {
        mocked.projectRules.mockResolvedValue(
          projectRules({ changes: [changeLine({ action: "set", rule: "presets", origin: null, id: null, text: "woocommerce", was: "woocommerce, yoast", table: null, undo: { op: "setPresets", presets: null }, edit: null })] }),
        );
        await openOverview();
        expect(await screen.findByRole("row", { name: "set presets woocommerce" })).toHaveTextContent("· auto woocommerce, yoast");
      });

      it.each([
        ["reset", "an edited rule", changeLine()],
        ["remove", "an added rule", changeLine({ action: "added", origin: null, id: "p1", undo: { op: "removeReference", id: "p1" } })],
        ["switch off", "a rule switched on", changeLine({ action: "switchedOn", undo: { op: "disable", id: "x" } })],
      ])("offers %s for %s and sends the line's undo", async (name, _what, line) => {
        mocked.projectRules.mockResolvedValue(projectRules({ changes: [line] }));
        await openOverview();
        await userEvent.click(await screen.findByRole("button", { name }));
        expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", line.undo);
      });

      it("opens the sheet of the rule a change edits", async () => {
        mocked.projectRules.mockResolvedValue(withReference({ changes: [changeLine()] }));
        await openOverview();
        await userEvent.click(await screen.findByRole("button", { name: "edit" }));
        const dialog = await screen.findByRole("dialog", { name: "edit reference" });
        expect(within(dialog).getByRole("combobox", { name: "Source table" })).toHaveValue("wp_wc_order_items");
      });

      it("offers no edit when the rule is not among the listed rules or the change has none", async () => {
        mocked.projectRules.mockResolvedValue(projectRules({ changes: [changeLine(), changeLine({ id: "y", edit: null })] }));
        await openOverview();
        await screen.findAllByRole("row", { name: `edited reference ${ORDER_ITEMS_ROW}` });
        expect(screen.queryByRole("button", { name: "edit" })).not.toBeInTheDocument();
      });

      it("opens the structure of the table the change is about", async () => {
        mocked.projectRules.mockResolvedValue(projectRules({ changes: [changeLine()] }));
        await openOverview();
        await userEvent.click(await screen.findByRole("button", { name: ORDER_ITEMS_ROW }));
        expect(activeTable()).toMatchObject({ kind: "table", database: "db", table: "wp_wc_order_items", view: "structure" });
      });
    });

    describe("problems", () => {
      beforeEach(() => mocked.projectRules.mockResolvedValue(withReference()));

      it("lists nothing for a reference whose values are all 0 or NULL", async () => {
        mocked.referenceCoverage.mockResolvedValue(coverage({ total: 4130, zeroOrNull: 4130 }));
        await openOverview();
        expect(await screen.findByText("No problems found.")).toBeInTheDocument();
        expect(screen.queryByRole("row", { name: ORDER_ITEMS_ROW })).not.toBeInTheDocument();
      });

      it("lists values that point nowhere with a sample of at most three", async () => {
        mocked.referenceCoverage.mockResolvedValue(coverage({ total: 40, resolved: 34, sampleUnresolved: ["98", "99", "100", "101"] }));
        await openOverview();
        expect(await problemRow()).toHaveTextContent(`${ORDER_ITEMS_ROW}: 6 of 40 point nowhere (98, 99, 100) · preset woocommerce`);
      });

      it("lists a reference that could not be counted", async () => {
        mocked.referenceCoverage.mockRejectedValue({ kind: "invalid_input", message: "Table is gone", code: null });
        await openOverview();
        expect(await problemRow()).toHaveTextContent(`${ORDER_ITEMS_ROW}: could not count: Table is gone`);
      });

      it("lists nothing for a reference that resolves", async () => {
        mocked.referenceCoverage.mockResolvedValue(coverage({ total: 7, resolved: 7 }));
        await openOverview();
        expect(await screen.findByText("No problems found.")).toBeInTheDocument();
        expect(screen.queryByRole("row", { name: ORDER_ITEMS_ROW })).not.toBeInTheDocument();
      });

      it("says how many references are counted until all answered", async () => {
        const pending = deferredCoverage();
        await openOverview();
        expect(await screen.findByText("counting 0 of 1 references…")).toBeInTheDocument();
        expect(screen.queryByText("No problems found.")).not.toBeInTheDocument();
        await act(async () => pending[0](coverage({ total: 7, resolved: 7 })));
        expect(await screen.findByText("No problems found.")).toBeInTheDocument();
      });

      it("counts again when Refresh coverage is clicked", async () => {
        mocked.referenceCoverage.mockResolvedValue(coverage({ total: 7, resolved: 5, sampleUnresolved: ["9"] }));
        await openOverview();
        expect(await problemRow()).toHaveTextContent("2 of 7 point nowhere (9)");
        mocked.referenceCoverage.mockResolvedValue(coverage({ total: 7, resolved: 7 }));
        await userEvent.click(screen.getByRole("button", { name: "Refresh coverage" }));
        expect(await screen.findByText("No problems found.")).toBeInTheDocument();
      });

      it("opens the rows that point nowhere with Open rows", async () => {
        mocked.referenceCoverage.mockResolvedValue(coverage({ total: 40, resolved: 35, sampleUnresolved: ["98"], rows: { unresolved: "`order_id` NOT IN (0)", zeroOrNull: "`order_id` = 0" } }));
        await openOverview();
        await userEvent.click(within(await problemRow()).getByRole("button", { name: "Open rows" }));
        expect(activeTable()).toMatchObject({ kind: "table", table: "wp_wc_order_items", view: "data", filter: "`order_id` NOT IN (0)" });
      });

      it("opens the table without a filter with Open table", async () => {
        mocked.referenceCoverage.mockResolvedValue(coverage({ total: 10, resolved: 5, zeroOrNull: 3, rows: { unresolved: "`order_id` NOT IN (0)", zeroOrNull: "`order_id` = 0" } }));
        await openOverview();
        await userEvent.click(within(await problemRow()).getByRole("button", { name: "Open table" }));
        expect(activeTable()).toMatchObject({ kind: "table", table: "wp_wc_order_items", view: "data" });
        expect(activeTable().filter).toBeUndefined();
      });

      it("offers only Open table when the counts carry no row filter", async () => {
        mocked.referenceCoverage.mockResolvedValue(coverage({ total: 10, resolved: 5 }));
        await openOverview();
        const row = within(await problemRow());
        expect(row.getByRole("button", { name: "Open table" })).toBeInTheDocument();
        expect(row.queryByRole("button", { name: "Open rows" })).not.toBeInTheDocument();
      });

      it("lists an anomaly whose query failed", async () => {
        const anomaly = { id: "orphans", preset: "woocommerce", text: "Orphans", count: null, address: null, filter: null, fix: null, error: "Unknown column 'x'" };
        mocked.readout.mockResolvedValue(readoutOf({ anomalies: { items: [anomaly, { ...anomaly, id: "fine", error: null, count: "0" }], error: null } }));
        await openOverview();
        expect(await screen.findByRole("row", { name: "anomaly orphans" })).toHaveTextContent("query failed: Unknown column 'x' · preset woocommerce");
        expect(screen.queryByRole("row", { name: "anomaly fine" })).not.toBeInTheDocument();
      });

      it("lists the error of the anomalies as a whole", async () => {
        mocked.readout.mockResolvedValue(readoutOf({ anomalies: { items: [], error: "Presets could not be read" } }));
        await openOverview();
        expect(await screen.findByRole("row", { name: "anomalies" })).toHaveTextContent("anomalies: could not run: Presets could not be read");
      });
    });

    describe("switching sections", () => {
      it("shows the per-table listing from All rules… and returns with Overview", async () => {
        mocked.projectRules.mockResolvedValue(withReference());
        await openOverview();
        await userEvent.click(screen.getByRole("button", { name: "All rules…" }));
        expect(await tableRegion()).toBeInTheDocument();
        expect(screen.getByRole("searchbox", { name: "Filter tables" })).toBeInTheDocument();
        expect(screen.queryByRole("region", { name: "Your changes" })).not.toBeInTheDocument();
        expect(screen.getByRole("button", { name: "All rules" })).toHaveAttribute("aria-pressed", "true");

        await userEvent.click(screen.getByRole("button", { name: "Overview" }));
        expect(await screen.findByRole("region", { name: "Your changes" })).toBeInTheDocument();
        expect(screen.queryByRole("searchbox", { name: "Filter tables" })).not.toBeInTheDocument();
      });

      it("shows the listing from the segmented control", async () => {
        await openOverview();
        const control = screen.getByRole("group", { name: "Project section" });
        await userEvent.click(within(control).getByRole("button", { name: "All rules" }));
        expect(await screen.findByRole("searchbox", { name: "Filter tables" })).toBeInTheDocument();
      });
    });
  });

  describe("tables", () => {
    const ordersGroup = tableRules("wp_wc_orders", {
      referencedBy: [{ from: { table: "wp_wc_order_items", columns: ["order_id"] }, to: { table: "wp_wc_orders", columns: ["id"] }, when: "", disabled: false }],
      entities: [{ preset: "woocommerce", name: "order", filter: null }],
      actions: [{ preset: "woocommerce", name: "mark_paid", scope: "row" }],
    });
    const fullItemsGroup = itemsGroup({ references: [referenceRow()], styles: [styleRow()], kinds: [kindRow()] });
    const looseGroup = tableRules(null, { kinds: [kindRow({ index: null, origin: { kind: "preset", preset: "woocommerce" }, rule: { columns: ["*.status"], enum: true } })] });

    beforeEach(() => mocked.projectRules.mockResolvedValue(projectRules({ groups: [ordersGroup, fullItemsGroup, looseGroup] })));

    it("renders one region per group, named by its table, in the order Rust returns them", async () => {
      await openProjectPage();
      await tableRegion();
      const page = screen.getByRole("region", { name: "Presets and prefix" }).parentElement!;
      const names = within(page).getAllByRole("region").map((region) => region.getAttribute("aria-label"));
      expect(names).toEqual(["Presets and prefix", "wp_wc_orders", "wp_wc_order_items", "no table"]);
    });

    it("counts a table's rules in its band", async () => {
      await openProjectPage();
      expect(within(await tableRegion()).getByText("1 reference · 1 value style · 1 kind")).toBeInTheDocument();
      expect(within(await tableRegion("wp_wc_orders")).getByText("1 incoming · 1 entity · 1 action")).toBeInTheDocument();
    });

    it("lists a table's references, value styles and kinds as rows", async () => {
      await openProjectPage();
      const region = within(await tableRegion());
      expect(region.getByRole("row", { name: ORDER_ITEMS_ROW })).toHaveTextContent("preset woocommerce");
      expect(region.getByRole("row", { name: STYLE_NAME })).toBeInTheDocument();
      expect(region.getByRole("row", { name: KIND_NAME })).toBeInTheDocument();
    });

    it("lists the incoming references, entities and actions of a table as read-only rows", async () => {
      await openProjectPage();
      const region = within(await tableRegion("wp_wc_orders"));
      expect(region.getByRole("row", { name: "referenced by" })).toHaveTextContent("referenced by wp_wc_order_items.order_id");
      const entity = region.getByRole("row", { name: "entity order" });
      expect(entity).toHaveTextContent("entity order · preset woocommerce");
      const action = region.getByRole("row", { name: "action mark_paid" });
      expect(action).toHaveTextContent("action mark_paid · row · preset woocommerce");
      expect(within(entity).queryByRole("button")).not.toBeInTheDocument();
      expect(within(action).queryByRole("button")).not.toBeInTheDocument();
    });

    it("collects rules that belong to no table in the no table group", async () => {
      await openProjectPage();
      expect(within(await tableRegion("no table")).getByRole("row", { name: "*.status enum" })).toBeInTheDocument();
    });

    it("hides a collapsed table's rows and remembers it when the page is shown again", async () => {
      const view = await openProjectPage();
      await userEvent.click(await screen.findByRole("button", { name: "Collapse wp_wc_order_items" }));
      expect(screen.queryByRole("row", { name: ORDER_ITEMS_ROW })).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Expand wp_wc_order_items" })).toHaveAttribute("aria-expanded", "false");

      view.unmount();
      render(<App />);
      expect(await screen.findByRole("button", { name: "Expand wp_wc_order_items" })).toBeInTheDocument();
      expect(screen.queryByRole("row", { name: ORDER_ITEMS_ROW })).not.toBeInTheDocument();
    });

    it("shows the rows again when the table is expanded", async () => {
      await openProjectPage();
      await userEvent.click(await screen.findByRole("button", { name: "Collapse wp_wc_order_items" }));
      await userEvent.click(screen.getByRole("button", { name: "Expand wp_wc_order_items" }));
      expect(screen.getByRole("row", { name: ORDER_ITEMS_ROW })).toBeInTheDocument();
    });

    describe("filter", () => {
      it("narrows the tables by a substring of the table name", async () => {
        await openProjectPage();
        expect(screen.getByText("3 tables with rules")).toBeInTheDocument();
        await userEvent.type(screen.getByRole("searchbox", { name: "Filter tables" }), "ITEMS");
        expect(screen.getByText("1 of 3 tables")).toBeInTheDocument();
        expect(screen.getByRole("region", { name: "wp_wc_order_items" })).toBeInTheDocument();
        expect(screen.queryByRole("region", { name: "wp_wc_orders" })).not.toBeInTheDocument();
        expect(screen.queryByRole("region", { name: "no table" })).not.toBeInTheDocument();
      });

      it("says so when no table matches", async () => {
        await openProjectPage();
        await userEvent.type(screen.getByRole("searchbox", { name: "Filter tables" }), "zzz");
        expect(screen.getByText("0 of 3 tables")).toBeInTheDocument();
        expect(screen.getByText("no table matches")).toBeInTheDocument();
      });
    });
  });

  describe("reference switches", () => {
    it.each([
      ["disables an enabled", false, { op: "disable", id: "x" }],
      ["enables a disabled", true, { op: "enable", id: "x" }],
    ])("%s reference by id", async (_what, disabled, change) => {
      mocked.projectRules.mockResolvedValue(rulesWith({ references: [referenceRow({ disabled })] }));
      await openProjectPage();
      const toggle = await screen.findByRole("switch", { name: ORDER_ITEMS_ROW });
      if (disabled) expect(toggle).not.toBeChecked();
      else expect(toggle).toBeChecked();
      await userEvent.click(toggle);
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", change);
    });

    it("gives a foreign key neither a switch nor an edit", async () => {
      mocked.projectRules.mockResolvedValue(rulesWith({ references: [foreignKeyRow()] }));
      await openProjectPage();
      const row = await screen.findByRole("row", { name: FK_ROW });
      expect(row).toHaveTextContent("foreign key fk_user");
      expect(within(row).getByText("not applied")).toBeInTheDocument();
      expect(within(row).queryByRole("switch")).not.toBeInTheDocument();
      expect(within(row).queryByRole("button")).not.toBeInTheDocument();
    });
  });

  describe("editing, resetting and removing references", () => {
    const rowIn = (name = ORDER_ITEMS_ROW) => screen.findByRole("row", { name });
    const showRow = async (row: ReferenceRow) => {
      mocked.projectRules.mockResolvedValue(rulesWith({ references: [row] }));
      await openProjectPage();
      return within(await rowIn());
    };

    it("offers edit and remove for a project reference and removes it by id", async () => {
      const row = await showRow(referenceRow({ origin: { kind: "project" }, id: "p1" }));
      expect(row.getByRole("button", { name: "edit" })).toBeInTheDocument();
      expect(row.queryByRole("button", { name: "reset" })).not.toBeInTheDocument();
      await userEvent.click(row.getByRole("button", { name: "remove" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "removeReference", id: "p1" });
    });

    it.each([
      ["preset", referenceRow({ id: "p1", edited: true }), "preset woocommerce"],
      ["inferred", inferredRow({ edited: true }), "inferred"],
    ])("offers edit and reset for an edited %s reference and resets it by id", async (_kind, edited, origin) => {
      const row = await showRow(edited);
      expect(row.getByText(new RegExp(origin))).toBeInTheDocument();
      expect(row.getByText(/· edited in project/)).toBeInTheDocument();
      expect(row.getByRole("button", { name: "edit" })).toBeInTheDocument();
      expect(row.queryByRole("button", { name: "remove" })).not.toBeInTheDocument();
      await userEvent.click(row.getByRole("button", { name: "reset" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "removeReference", id: edited.id });
    });

    it.each([
      ["preset", referenceRow()],
      ["inferred", inferredRow()],
    ])("offers only edit for an unedited %s reference", async (_kind, unedited) => {
      const row = await showRow(unedited);
      expect(row.getByRole("button", { name: "edit" })).toBeInTheDocument();
      expect(row.queryByRole("button", { name: "reset" })).not.toBeInTheDocument();
      expect(row.queryByRole("button", { name: "remove" })).not.toBeInTheDocument();
      expect(row.queryByText(/edited in project/)).not.toBeInTheDocument();
    });

    it.each([
      ["a project reference", referenceRow({ origin: { kind: "project" }, id: "p1" })],
      ["an edited preset reference", referenceRow({ id: "p1", edited: true })],
    ])("saves an edit of %s as an update of its id", async (_kind, row) => {
      const shown = await showRow(row);
      await userEvent.click(shown.getByRole("button", { name: "edit" }));
      const dialog = await screen.findByRole("dialog", { name: "edit reference" });
      expect(within(dialog).getByRole("combobox", { name: "Source table" })).toHaveValue("wp_wc_order_items");
      await userEvent.click(within(dialog).getByRole("button", { name: "Save to project" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "updateReference", id: "p1", reference: expect.objectContaining({ from: "wc_order_items.order_id", to: "wc_orders.id" }) });
    });

    it("saves an edit of an unedited preset reference as a project rule that overrides the preset's id", async () => {
      const row = await showRow(referenceRow());
      await userEvent.click(row.getByRole("button", { name: "edit" }));
      const dialog = await screen.findByRole("dialog", { name: "edit reference" });
      await userEvent.click(within(dialog).getByRole("button", { name: "Save to project" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "addReference", reference: expect.objectContaining({ id: "x", from: "wc_order_items.order_id", to: "wc_orders.id" }) });
    });

    it("saves an edit of an inferred reference as a new project rule with no id", async () => {
      const row = await showRow(inferredRow());
      await userEvent.click(row.getByRole("button", { name: "edit" }));
      const dialog = await screen.findByRole("dialog", { name: "edit reference" });
      await userEvent.click(within(dialog).getByRole("button", { name: "Save to project" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "addReference", reference: expect.objectContaining({ id: "", from: "wc_order_items.order_id" }) });
    });
  });

  describe("reference coverage", () => {
    beforeEach(() => mocked.projectRules.mockResolvedValue(rulesWith({ references: [referenceRow()] })));

    it("counts every applied reference on open without a button to ask for it", async () => {
      const second = itemsReference("order_item_id");
      mocked.projectRules.mockResolvedValue(rulesWith({ references: [referenceRow(), second, foreignKeyRow()] }));
      await openProjectPage();
      await waitFor(() => expect(mocked.referenceCoverage).toHaveBeenCalledTimes(2));
      expect(mocked.referenceCoverage.mock.calls.map(([, , reference]) => reference)).toEqual([referenceRow().reference, second.reference]);
      expect(mocked.referenceCoverage).toHaveBeenCalledWith("s1", "db", referenceRow().reference);
      expect(screen.queryByRole("button", { name: "check" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Check coverage for all" })).not.toBeInTheDocument();
    });

    it("says counting until the answer arrives", async () => {
      deferredCoverage();
      await openProjectPage();
      expect(await within(await screen.findByRole("row", { name: ORDER_ITEMS_ROW })).findByText("counting…")).toBeInTheDocument();
    });

    it("shows resolved of total in the row and what the counts leave out in the tooltip", async () => {
      mocked.referenceCoverage.mockResolvedValue(coverage({ total: 7, resolved: 6, zeroOrNull: 1, sampleUnresolved: ["9"] }));
      await openProjectPage();
      const row = await screen.findByRole("row", { name: ORDER_ITEMS_ROW });
      const count = await within(row).findByText("6 of 7");
      expect(count.closest("[title]")).toHaveAttribute("title", "1 is 0 or NULL · unresolved 9");
    });

    it("says so when the count timed out", async () => {
      mocked.referenceCoverage.mockResolvedValue(coverage({ timedOut: true }));
      await openProjectPage();
      expect(await within(await screen.findByRole("row", { name: ORDER_ITEMS_ROW })).findByText("timed out")).toBeInTheDocument();
    });

    it("shows the error of a failed count in the row", async () => {
      mocked.referenceCoverage.mockRejectedValue({ kind: "invalid_input", message: "Table is gone", code: null });
      await openProjectPage();
      expect(await within(await screen.findByRole("row", { name: ORDER_ITEMS_ROW })).findByText("Table is gone")).toBeInTheDocument();
    });

    describe("queue", () => {
      const five = ["a", "b", "c", "d", "e"].map((column) => itemsReference(column));

      beforeEach(() => mocked.projectRules.mockResolvedValue(rulesWith({ references: five })));

      it("runs at most three counts at once and starts the next as one finishes", async () => {
        const pending = deferredCoverage();
        await openProjectPage();
        await waitFor(() => expect(mocked.referenceCoverage).toHaveBeenCalledTimes(3));
        await act(async () => {});
        expect(mocked.referenceCoverage).toHaveBeenCalledTimes(3);

        await act(async () => pending[0](coverage({ total: 1, resolved: 1 })));
        await waitFor(() => expect(mocked.referenceCoverage).toHaveBeenCalledTimes(4));
        await act(async () => {});
        expect(mocked.referenceCoverage).toHaveBeenCalledTimes(4);
      });

      it("stops counting when the page is left", async () => {
        const pending = deferredCoverage();
        const view = await openProjectPage();
        await waitFor(() => expect(mocked.referenceCoverage).toHaveBeenCalledTimes(3));
        view.unmount();
        await act(async () => pending.forEach((resolve) => resolve(coverage({ total: 1, resolved: 1 }))));
        expect(mocked.referenceCoverage).toHaveBeenCalledTimes(3);
      });
    });

    it("keeps the counts for the session, so showing the page again counts nothing", async () => {
      mocked.referenceCoverage.mockResolvedValue(coverage({ total: 7, resolved: 6 }));
      const view = await openProjectPage();
      await screen.findByText("6 of 7");
      view.unmount();
      vi.clearAllMocks();

      render(<App />);
      expect(await screen.findByText("6 of 7")).toBeInTheDocument();
      expect(mocked.referenceCoverage).not.toHaveBeenCalled();
    });

    it("counts again when Refresh coverage is clicked", async () => {
      mocked.referenceCoverage.mockResolvedValue(coverage({ total: 7, resolved: 6 }));
      await openProjectPage();
      await screen.findByText("6 of 7");
      mocked.referenceCoverage.mockResolvedValue(coverage({ total: 8, resolved: 8 }));
      await userEvent.click(screen.getByRole("button", { name: "Refresh coverage" }));
      expect(await screen.findByText("8 of 8")).toBeInTheDocument();
      expect(screen.queryByText("6 of 7")).not.toBeInTheDocument();
    });
  });

  describe("the edit sheet", () => {
    const form = (): ReferenceForm => ({
      ...referenceFormFor(paidViaSource, { source: "wc_orders_meta.meta_value", meta: null }),
      target: { table: "wp_wc_orders", column: "id" },
    });

    it("opens as a dialog named after the form", async () => {
      await openProjectPage(form());
      const dialog = await screen.findByRole("dialog", { name: "new reference" });
      expect(within(dialog).getByRole("combobox", { name: "Target table" })).toHaveValue("wp_wc_orders");
    });

    it("saves on ⌘↵ from any field and closes", async () => {
      await openProjectPage(form());
      const dialog = await screen.findByRole("dialog", { name: "new reference" });
      await userEvent.click(within(dialog).getByRole("textbox", { name: "Label" }));
      await userEvent.keyboard("{Meta>}{Enter}{/Meta}");
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "addReference", reference: expect.objectContaining({ from: "wc_orders_meta.meta_value", to: "wc_orders.id" }) });
      await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    });

    it("closes on Escape without saving", async () => {
      await openProjectPage(form());
      await screen.findByRole("dialog", { name: "new reference" });
      await userEvent.keyboard("{Escape}");
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(mocked.projectUpdate).not.toHaveBeenCalled();
    });

    it("closes on Cancel without saving", async () => {
      await openProjectPage(form());
      await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      expect(mocked.projectUpdate).not.toHaveBeenCalled();
    });

    it("shows the Rust error under the field it names and keeps the sheet open", async () => {
      mocked.projectUpdate.mockRejectedValue({ kind: "invalid_input", message: "`reference.to`: `wp_users` has no column `nope`", code: null, field: { path: "reference.to", message: "`wp_users` has no column `nope`" } });
      await openProjectPage({ ...form(), target: { table: "wp_users", column: "nope" } });
      const dialog = await screen.findByRole("dialog", { name: "new reference" });
      await userEvent.click(within(dialog).getByRole("button", { name: "Save to project" }));
      const alert = await within(dialog).findByRole("alert");
      expect(alert).toHaveTextContent("`wp_users` has no column `nope`");
      expect(alert.parentElement).toContainElement(within(dialog).getByRole("combobox", { name: "Target table" }));
      expect(screen.getByRole("dialog", { name: "new reference" })).toBeInTheDocument();
    });

    it("asks for a target while none is picked and saves nothing", async () => {
      await openProjectPage({ ...form(), target: { table: "", column: "" } });
      const dialog = await screen.findByRole("dialog", { name: "new reference" });
      await userEvent.click(within(dialog).getByRole("button", { name: "Save to project" }));
      expect(await within(dialog).findByRole("alert")).toHaveTextContent("pick a target table and column");
      expect(mocked.projectUpdate).not.toHaveBeenCalled();
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
  });

  describe("adding a reference from a meta value", () => {
    const draft: RuleDraft = { source: "wc_orders_meta.meta_value", meta: { table: "wc_orders_meta", keyColumn: "meta_key", key: "_ppcp_paypal_order_id" } };

    beforeEach(() => mocked.ruleDraft.mockResolvedValue(draft));

    it("opens the sheet on the Project tab, filled from the value, and saves the rule", async () => {
      await openOrderObject();
      await userEvent.click(screen.getByText("bacs"));
      await userEvent.keyboard("r");

      const dialog = await screen.findByRole("dialog", { name: "new reference" });
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "project" });
      expect(mocked.ruleDraft).toHaveBeenCalledWith("s1", "db", paidViaSource);
      expect(within(dialog).getByRole("combobox", { name: "Source table" })).toHaveValue("wp_wc_orders_meta");
      expect(within(dialog).getByRole("combobox", { name: "When column" })).toHaveValue("meta_key");
      expect(within(dialog).getByRole("textbox", { name: "When value" })).toHaveValue("_ppcp_paypal_order_id");

      await userEvent.selectOptions(within(dialog).getByRole("combobox", { name: "Target table" }), "wp_wc_orders");
      await waitFor(() => expect(within(dialog).getByRole("combobox", { name: "Target column" })).toHaveValue("id"));
      await userEvent.click(within(dialog).getByRole("button", { name: "Save to project" }));

      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", {
        op: "addReference",
        reference: expect.objectContaining({ from: "wc_orders_meta.meta_value", when: { meta_key: "_ppcp_paypal_order_id" }, to: "wc_orders.id" }),
      });
      await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    });

    it("opens the same sheet from the Add reference… menu item", async () => {
      await openOrderObject();
      fireEvent.contextMenu(screen.getByText("bacs"));
      await userEvent.click(screen.getByRole("menuitem", { name: "Add reference…" }));
      const dialog = await screen.findByRole("dialog", { name: "new reference" });
      expect(selectActiveTab(useStore.getState())).toMatchObject({ kind: "project" });
      expect(within(dialog).getByRole("textbox", { name: "When value" })).toHaveValue("_ppcp_paypal_order_id");
    });
  });

  describe("value styles", () => {
    beforeEach(() => mocked.ruleDraft.mockResolvedValue({ source: "wc_orders.status", meta: null }));

    it("takes the value's column, saves the chosen match, text and color, and lists the style", async () => {
      await openOrderObject();
      fireEvent.contextMenu(screen.getByText("wc-refunded"));
      await userEvent.click(screen.getByRole("menuitem", { name: "Style…" }));

      const dialog = await screen.findByRole("dialog", { name: "new value style" });
      expect(within(dialog).getByRole("textbox", { name: "columns" })).toHaveValue("wc_orders.status");
      const value = within(dialog).getByRole("textbox", { name: "Value" });
      expect(value).toHaveValue("wc-refunded");
      await userEvent.clear(value);
      await userEvent.type(value, "wc-failed");
      expect(within(dialog).getByRole("button", { name: "red" })).toHaveAttribute("aria-pressed", "true");
      expect(within(dialog).getByRole("button", { name: "blue" })).toHaveAttribute("aria-pressed", "false");

      respondToChangeWith(projectRules({ groups: [tableRules("wp_wc_orders", { styles: [styleRow()] })] }));
      await userEvent.click(within(dialog).getByRole("button", { name: "Save to project" }));

      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "addStyle", rule: expect.objectContaining({ columns: ["wc_orders.status"], match: "=", value: "wc-failed", color: "red", enabled: true }) });
      await userEvent.click(await screen.findByRole("button", { name: "All rules…" }));
      const region = await tableRegion("wp_wc_orders");
      expect(await within(region).findByRole("row", { name: STYLE_NAME })).toBeInTheDocument();
    });

    it("moves the pressed state when another color is picked", async () => {
      await openOrderObject();
      fireEvent.contextMenu(screen.getByText("wc-refunded"));
      await userEvent.click(screen.getByRole("menuitem", { name: "Style…" }));
      const dialog = await screen.findByRole("dialog", { name: "new value style" });
      await userEvent.click(within(dialog).getByRole("button", { name: "green" }));
      expect(within(dialog).getByRole("button", { name: "green" })).toHaveAttribute("aria-pressed", "true");
      expect(within(dialog).getByRole("button", { name: "red" })).toHaveAttribute("aria-pressed", "false");
    });

    describe("rows", () => {
      const open = async (row: StyleRow) => {
        mocked.projectRules.mockResolvedValue(rulesWith({ styles: [row] }));
        await openProjectPage();
        return within(await screen.findByRole("row", { name: STYLE_NAME }));
      };

      it.each([
        ["a project style that is on", styleRow(), true, "disableStyle"],
        ["a project style stored off", styleRow({ disabled: true }), false, "enableStyle"],
        ["a preset style that is on", presetStyle(), true, "disableStyle"],
        ["a preset style that is off", presetStyle({ disabled: true }), false, "enableStyle"],
      ])("switches %s by id", async (_what, row, checked, op) => {
        await open(row);
        const toggle = screen.getByRole("switch", { name: STYLE_NAME });
        if (checked) expect(toggle).toBeChecked();
        else expect(toggle).not.toBeChecked();
        await userEvent.click(toggle);
        expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op, id: STYLE_ID });
      });

      it("removes a project style by id", async () => {
        const row = await open(styleRow());
        expect(row.queryByRole("button", { name: "reset" })).not.toBeInTheDocument();
        await userEvent.click(row.getByRole("button", { name: "remove" }));
        expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "removeStyle", id: STYLE_ID });
      });

      it("marks an edited preset style and resets it by id", async () => {
        const row = await open(presetStyle({ edited: true }));
        expect(row.getByText(/edited in project/)).toBeInTheDocument();
        expect(row.queryByRole("button", { name: "remove" })).not.toBeInTheDocument();
        await userEvent.click(row.getByRole("button", { name: "reset" }));
        expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "removeStyle", id: STYLE_ID });
      });

      it("offers neither reset nor remove for a preset style that is not edited", async () => {
        const row = await open(presetStyle());
        expect(row.queryByRole("button", { name: "reset" })).not.toBeInTheDocument();
        expect(row.queryByRole("button", { name: "remove" })).not.toBeInTheDocument();
      });

      it.each([
        ["a project style", styleRow()],
        ["a preset style", presetStyle()],
      ])("saves an edit of %s as an update of its id", async (_what, row) => {
        const shown = await open(row);
        await userEvent.click(shown.getByRole("button", { name: "edit" }));
        const dialog = await screen.findByRole("dialog", { name: "edit value style" });
        expect(within(dialog).getByRole("textbox", { name: "columns" })).toHaveFocus();
        const value = within(dialog).getByRole("textbox", { name: "Value" });
        await userEvent.clear(value);
        await userEvent.type(value, "wc-cancelled");
        await userEvent.click(within(dialog).getByRole("button", { name: "Save to project" }));
        expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "updateStyle", id: STYLE_ID, rule: expect.objectContaining({ columns: ["wc_orders.status"], value: "wc-cancelled", color: "red" }) });
      });
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
      const dialog = await screen.findByRole("dialog", { name: "new kind" });
      expect(within(dialog).getByRole("textbox", { name: "meta keys" })).toHaveValue("_paid_via");
      expect(within(dialog).getByRole("textbox", { name: "in meta tables" })).toHaveValue("wc_orders_meta");
      await userEvent.selectOptions(within(dialog).getByRole("combobox", { name: "Kind" }), "secret");
      await userEvent.click(within(dialog).getByRole("button", { name: "Save to project" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "addKind", rule: { columns: [], meta_keys: ["_paid_via"], in: ["wc_orders_meta"], secret: true } });
    });

    describe("rows", () => {
      it("updates a project kind by index with the edited rule", async () => {
        mocked.projectRules.mockResolvedValue(rulesWith({ kinds: [kindRow()] }));
        await openProjectPage();
        await userEvent.click(within(await screen.findByRole("row", { name: KIND_NAME })).getByRole("button", { name: "edit" }));
        const dialog = await screen.findByRole("dialog", { name: "edit kind" });
        await userEvent.selectOptions(within(dialog).getByRole("combobox", { name: "Kind" }), "number");
        await userEvent.click(within(dialog).getByRole("button", { name: "Save to project" }));
        expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "updateKind", index: 2, rule: expect.objectContaining({ columns: ["wc_orders.status"], number: true }) });
      });

      it("removes a project kind by index", async () => {
        mocked.projectRules.mockResolvedValue(rulesWith({ kinds: [kindRow()] }));
        await openProjectPage();
        await userEvent.click(within(await screen.findByRole("row", { name: KIND_NAME })).getByRole("button", { name: "remove" }));
        expect(mocked.projectUpdate).toHaveBeenCalledWith("s1", "db", { op: "removeKind", index: 2 });
      });

      it("shows a preset kind without edit or remove", async () => {
        mocked.projectRules.mockResolvedValue(rulesWith({ kinds: [kindRow({ index: null, origin: { kind: "preset", preset: "woocommerce" } })] }));
        await openProjectPage();
        const row = await screen.findByRole("row", { name: KIND_NAME });
        expect(row).toHaveTextContent("preset woocommerce");
        expect(within(row).queryByRole("button")).not.toBeInTheDocument();
      });
    });
  });

  describe("long rule lists", () => {
    const pad = (n: number) => String(n).padStart(4, "0");

    it("renders only a window of 500 references", async () => {
      mocked.referenceCoverage.mockImplementation(() => new Promise<Coverage>(() => {}));
      const references = Array.from({ length: 500 }, (_, i) => itemsReference(`ref_${pad(i + 1)}`));
      mocked.projectRules.mockResolvedValue(rulesWith({ references }));
      await openProjectPage();
      const section = within(await tableRegion());
      expect(await section.findByText("ref_0001")).toBeInTheDocument();
      expect(section.queryByText("ref_0500")).not.toBeInTheDocument();
      expect(section.getAllByRole("row").length).toBeLessThan(200);
    });

    it("renders only a window of 500 kinds", async () => {
      const kinds = Array.from({ length: 500 }, (_, i) => kindRow({ index: i, rule: { columns: [`kind_${pad(i + 1)}`], secret: true } }));
      mocked.projectRules.mockResolvedValue(rulesWith({ kinds }));
      await openProjectPage();
      const section = within(await tableRegion());
      expect(await section.findByText("kind_0001")).toBeInTheDocument();
      expect(section.queryByText("kind_0500")).not.toBeInTheDocument();
      expect(section.getAllByRole("row").length).toBeLessThan(200);
    });
  });
});
