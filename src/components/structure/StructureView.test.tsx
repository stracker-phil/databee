import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../../test/apiMock")).createApiMock() }));

import { api, type ColumnProfile, type KindRow, type ReferenceRow, type StyleRow, type TableProfile } from "../../api";
import { useStore } from "../../state/store";
import { connectedStore, tableTab } from "../../test/fixtures";
import { StructureView } from "./StructureView";

const mocked = vi.mocked(api, true);

const tab = tableTab({ view: "structure" });

const column = (name: string, patch: Partial<ColumnProfile> = {}, columnType = "bigint unsigned"): ColumnProfile => ({
  column: { name, columnType, nullable: false, default: null, key: "", extra: "", comment: "", autoIncrement: false, generated: false },
  primaryKey: false,
  indexes: [],
  kinds: [],
  references: [],
  styles: [],
  ...patch,
});

const preset = { kind: "preset", preset: "wordpress" } as const;

function reference(patch: Partial<ReferenceRow> = {}): ReferenceRow {
  const from = { table: "wp_posts", columns: ["post_author"] };
  const to = { table: "wp_users", columns: ["ID"] };
  return {
    origin: preset,
    id: "wp.author",
    from,
    to,
    when: "",
    toWhen: "",
    label: null,
    disabled: false,
    edited: false,
    reference: { from, to, by: { kind: "preset", preset: "wordpress", id: "wp.author" } },
    rule: { id: "wp.author", from: "posts.post_author", to: "users.ID" },
    ...patch,
  };
}

const kind = (patch: Partial<KindRow> = {}): KindRow => ({ origin: preset, index: null, rule: { columns: ["*.*_gmt"], timestamp: "utc" }, ...patch });

const style = (patch: Partial<StyleRow> = {}): StyleRow => ({
  origin: preset,
  id: "wp.status",
  rule: { id: "wp.status", columns: ["posts.post_status"], match: "=", value: "publish", color: "green", enabled: true },
  disabled: false,
  edited: false,
  ...patch,
});

function profile(patch: Partial<TableProfile> = {}): TableProfile {
  return { table: "wp_posts", ruleName: "posts", columns: [], meta: [], referencedBy: [], entities: [], actions: [], ddl: null, ...patch };
}

const showProfile = (patch: Partial<TableProfile>) => mocked.tableRules.mockResolvedValue(profile(patch));

const row = (name: string) => screen.findByRole("row", { name });

describe("StructureView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore({ tabs: [tab] });
  });

  it("shows the error when the profile cannot be loaded", async () => {
    mocked.tableRules.mockRejectedValue({ kind: "database", message: "Table is locked", code: null });
    render(<StructureView tab={tab} />);
    expect(await screen.findByText("Table is locked")).toBeInTheDocument();
  });

  describe("columns", () => {
    it("lists one row per column with its type, primary key and index marks", async () => {
      showProfile({
        columns: [
          column("ID", { primaryKey: true, indexes: [{ name: "PRIMARY", unique: true, position: 1 }] }),
          column("post_status", { indexes: [{ name: "type_status", unique: false, position: 2 }] }, "varchar(20)"),
          column("post_name", { indexes: [{ name: "post_name_key", unique: true, position: 1 }] }, "varchar(200)"),
        ],
      });
      render(<StructureView tab={tab} />);

      const id = await row("ID");
      expect(within(id).getByText("bigint unsigned")).toBeInTheDocument();
      expect(within(id).getByText("PK")).toBeInTheDocument();
      expect(id).not.toHaveTextContent("PRIMARY");

      const status = await row("post_status");
      expect(within(status).getByText("varchar(20)")).toBeInTheDocument();
      expect(within(status).getByText("not null")).toBeInTheDocument();
      expect(status).toHaveTextContent("idx type_status 2");

      expect(await row("post_name")).toHaveTextContent("UNI post_name_key");
    });
  });

  describe("kinds", () => {
    it("shows the kind a glob gives with the preset and the pattern", async () => {
      showProfile({ columns: [column("post_date_gmt", { kinds: [{ row: kind(), pattern: "*.*_gmt", applies: true }] }, "datetime")] });
      render(<StructureView tab={tab} />);
      const group = within(await row("post_date_gmt")).getByRole("group", { name: "kind timestamp · utc" });
      expect(group).toHaveTextContent("preset wordpress: *.*_gmt");
      expect(group).not.toHaveTextContent("overridden");
    });

    it("marks a kind another rule overrides", async () => {
      showProfile({ columns: [column("post_date_gmt", { kinds: [{ row: kind(), pattern: "*.*_gmt", applies: false }] }, "datetime")] });
      render(<StructureView tab={tab} />);
      expect(within(await row("post_date_gmt")).getByRole("group", { name: "kind timestamp · utc" })).toHaveTextContent("overridden");
    });
  });

  describe("references", () => {
    it("shows the target, the condition, the origin and the coverage of a column's reference", async () => {
      mocked.referenceCoverage.mockResolvedValue({ total: 7, resolved: 6, zeroOrNull: 1, sampleUnresolved: [], timedOut: false });
      showProfile({ columns: [column("post_parent", { references: [reference({ id: "wp.parent", from: { table: "wp_posts", columns: ["post_parent"] }, when: "post_type = 'revision'" })] })] });
      render(<StructureView tab={tab} />);

      const group = within(await row("post_parent")).getByRole("group", { name: "wp_posts.post_parent post_type = 'revision' → wp_users.ID" });
      expect(group).toHaveTextContent("→ wp_users.ID when this row has post_type = 'revision'");
      expect(group).toHaveTextContent("preset wordpress");
      expect(await within(group).findByText("6 of 7")).toBeInTheDocument();
    });

    it("keeps the target's condition apart from the row's", async () => {
      showProfile({ columns: [column("post_author", { references: [reference({ toWhen: "user_status = '0'", when: "post_type = 'post'" })] })] });
      render(<StructureView tab={tab} />);

      expect(within(await row("post_author")).getByRole("group")).toHaveTextContent("→ wp_users.ID (rows with user_status = '0') when this row has post_type = 'post'");
    });

    it("reads a reference within the table as other rows with the same value", async () => {
      const to = { table: "wp_posts", columns: ["post_name"] };
      showProfile({ columns: [column("post_name", { references: [reference({ from: to, to, when: "post_parent = '0'", toWhen: "post_parent = '0'" })] }, "varchar(200)")] });
      render(<StructureView tab={tab} />);

      expect(within(await row("post_name")).getByRole("group")).toHaveTextContent("→ other wp_posts rows with the same post_name and post_parent = '0' when this row has post_parent = '0'");
    });

    it("names the target column of a reference to another column of the same table", async () => {
      showProfile({ columns: [column("post_parent", { references: [reference({ from: { table: "wp_posts", columns: ["post_parent"] }, to: { table: "wp_posts", columns: ["ID"] } })] })] });
      render(<StructureView tab={tab} />);

      expect(within(await row("post_parent")).getByRole("group")).toHaveTextContent("→ wp_posts.ID");
    });

    it("offers a switch for a preset reference but none for a foreign key", async () => {
      const fk = reference({
        origin: { kind: "foreignKey", constraint: "fk_term" },
        id: null,
        from: { table: "wp_posts", columns: ["term_id"] },
        to: { table: "wp_terms", columns: ["term_id"] },
        rule: null,
      });
      showProfile({ columns: [column("post_author", { references: [reference()] }), column("term_id", { references: [fk] })] });
      render(<StructureView tab={tab} />);

      expect(await screen.findByRole("switch", { name: "wp_posts.post_author → wp_users.ID" })).toBeChecked();
      const fkGroup = within(await row("term_id")).getByRole("group", { name: "wp_posts.term_id → wp_terms.term_id" });
      expect(fkGroup).toHaveTextContent("foreign key fk_term");
      expect(within(fkGroup).queryByRole("switch")).not.toBeInTheDocument();
    });

    it("sends disable when a preset reference is switched off", async () => {
      showProfile({ columns: [column("post_author", { references: [reference()] })] });
      render(<StructureView tab={tab} />);
      await userEvent.click(await screen.findByRole("switch", { name: "wp_posts.post_author → wp_users.ID" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith(tab.sessionId, tab.database, { op: "disable", id: "wp.author" });
    });

    it("sends enable when a switched off reference is switched on", async () => {
      showProfile({ columns: [column("post_author", { references: [reference({ disabled: true, reference: null })] })] });
      render(<StructureView tab={tab} />);
      const toggle = await screen.findByRole("switch", { name: "wp_posts.post_author → wp_users.ID" });
      expect(toggle).not.toBeChecked();
      await userEvent.click(toggle);
      expect(mocked.projectUpdate).toHaveBeenCalledWith(tab.sessionId, tab.database, { op: "enable", id: "wp.author" });
    });

    it("resets an edited preset reference", async () => {
      showProfile({ columns: [column("post_author", { references: [reference({ id: "project-1", edited: true })] })] });
      render(<StructureView tab={tab} />);
      const group = within(await row("post_author")).getByRole("group");
      expect(group).toHaveTextContent("edited in project");
      await userEvent.click(await within(group).findByRole("button", { name: "reset" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith(tab.sessionId, tab.database, { op: "removeReference", id: "project-1" });
    });

    it("opens the rule sheet to edit a reference", async () => {
      showProfile({ columns: [column("post_author", { references: [reference()] })] });
      render(<StructureView tab={tab} />);
      await userEvent.click(await within(await row("post_author")).findByRole("button", { name: "edit" }));
      expect(await screen.findByRole("dialog", { name: "edit reference" })).toBeInTheDocument();
    });
  });

  describe("value styles", () => {
    it("shows the match, the color and a switch", async () => {
      showProfile({ columns: [column("post_status", { styles: [{ row: style(), pattern: "posts.post_status" }] }, "varchar(20)")] });
      render(<StructureView tab={tab} />);
      const group = within(await row("post_status")).getByRole("group", { name: "style = 'publish' green" });
      expect(group).toHaveTextContent("= 'publish'");
      expect(group).toHaveTextContent("green");
      expect(await within(group).findByRole("switch")).toBeChecked();
    });

    it.each([
      { disabled: false, op: "disableStyle" },
      { disabled: true, op: "enableStyle" },
    ])("sends $op when the style is disabled: $disabled", async ({ disabled, op }) => {
      showProfile({ columns: [column("post_status", { styles: [{ row: style({ disabled }), pattern: null }] }, "varchar(20)")] });
      render(<StructureView tab={tab} />);
      await userEvent.click(await screen.findByRole("switch", { name: "style = 'publish' green" }));
      expect(mocked.projectUpdate).toHaveBeenCalledWith(tab.sessionId, tab.database, { op, id: "wp.status" });
    });
  });

  describe("meta keys", () => {
    it("lists the meta table's references, kinds and styles by their scope", async () => {
      const metaReference = reference({
        id: "wp.thumb",
        from: { table: "wp_postmeta", columns: ["meta_value"] },
        to: { table: "wp_posts", columns: ["ID"] },
        when: "meta_key = '_thumbnail_id'",
      });
      showProfile({
        table: "wp_postmeta",
        meta: [
          {
            table: "wp_postmeta",
            keyColumn: "meta_key",
            valueColumn: "meta_value",
            references: [metaReference],
            kinds: [kind({ rule: { meta_keys: ["_price"], money: {} } })],
            styles: [style({ id: "wp.flag", rule: { id: "wp.flag", meta_keys: ["_flag"], match: "=", value: "yes", color: "red", enabled: true } })],
          },
        ],
      });
      render(<StructureView tab={{ ...tab, table: "wp_postmeta" }} />);

      const heading = await screen.findByRole("heading", { name: /meta keys · wp_postmeta/ });
      const block = within(heading.closest("section") as HTMLElement);
      expect(heading).toHaveTextContent("(meta_key → meta_value)");
      expect(block.getByText("meta_key = '_thumbnail_id'")).toBeInTheDocument();
      expect(block.getByRole("group", { name: "wp_postmeta.meta_value → wp_posts.ID" })).toHaveTextContent("→ wp_posts.ID");
      expect(block.getByText("meta keys _price")).toBeInTheDocument();
      expect(block.getByRole("group", { name: "kind money" })).toBeInTheDocument();
      expect(block.getByText("meta keys _flag")).toBeInTheDocument();
      expect(block.getByRole("group", { name: "style = 'yes' red" })).toBeInTheDocument();
    });

    it("says no rule names a key when the meta table has none", async () => {
      showProfile({ meta: [{ table: "wp_postmeta", keyColumn: "meta_key", valueColumn: "meta_value", references: [], kinds: [], styles: [] }] });
      render(<StructureView tab={tab} />);
      expect(await screen.findByText("no rule names a meta key here")).toBeInTheDocument();
    });
  });

  describe("table blocks", () => {
    it("lists the references pointing at the table by their source", async () => {
      showProfile({
        referencedBy: [reference({ id: "wp.meta", from: { table: "wp_postmeta", columns: ["post_id"] }, to: { table: "wp_posts", columns: ["ID"] } })],
      });
      render(<StructureView tab={tab} />);
      const block = within(await screen.findByRole("region", { name: "Referenced by" }));
      expect(block.getByRole("button", { name: "wp_postmeta.post_id" })).toBeInTheDocument();
      expect(block.getByRole("group")).toHaveTextContent("← wp_postmeta.post_id");
    });

    it("lists entities with their filter, summary and created column", async () => {
      showProfile({ entities: [{ preset: "wordpress", name: "post", filter: "post_type = 'post'", summary: ["post_title", "post_status"], created: "post_date" }] });
      render(<StructureView tab={tab} />);
      const entity = within(await screen.findByRole("region", { name: "Entities" })).getByRole("group", { name: "entity post" });
      expect(entity).toHaveTextContent("where post_type = 'post'");
      expect(entity).toHaveTextContent("summary post_title, post_status");
      expect(entity).toHaveTextContent("created post_date");
      expect(entity).toHaveTextContent("preset wordpress");
    });

    it("lists actions with their scope and what they write", async () => {
      showProfile({ actions: [{ preset: "wordpress", name: "Publish", scope: "row", writes: [{ write: "set", table: "wp_posts", columns: ["post_status"] }] }] });
      render(<StructureView tab={tab} />);
      const action = within(await screen.findByRole("region", { name: "Actions" })).getByRole("group", { name: "action Publish" });
      expect(action).toHaveTextContent("row");
      expect(action).toHaveTextContent("wp_posts: post_status");
      expect(action).toHaveTextContent("preset wordpress");
    });

    it("omits blocks without content", async () => {
      showProfile({ columns: [column("ID")] });
      render(<StructureView tab={tab} />);
      await row("ID");
      for (const name of ["Referenced by", "Entities", "Actions"]) expect(screen.queryByRole("region", { name })).not.toBeInTheDocument();
    });
  });

  describe("column menu", () => {
    beforeEach(async () => {
      showProfile({ columns: [column("post_author", { references: [reference()] })] });
      useStore.setState({ tabs: [tab], activeTabId: tab.id });
      render(<StructureView tab={tab} />);
      await screen.findByRole("switch");
    });

    it("offers the rules to add and opening the rows", async () => {
      await userEvent.click(screen.getByRole("button", { name: "Column menu post_author" }));
      const menu = within(await screen.findByRole("menu"));
      for (const name of ["Add reference…", "Add value style…", "Add kind…", "Open rows"]) expect(menu.getByRole("menuitem", { name })).toBeEnabled();
    });

    it("opens from a right-click on the row", async () => {
      fireEvent.contextMenu(await row("post_author"));
      expect(await screen.findByRole("menuitem", { name: "Add kind…" })).toBeInTheDocument();
    });

    it("opens the sheet for a new value style", async () => {
      await userEvent.click(screen.getByRole("button", { name: "Column menu post_author" }));
      await userEvent.click(await screen.findByRole("menuitem", { name: "Add value style…" }));
      expect(await screen.findByRole("dialog", { name: "new value style" })).toBeInTheDocument();
    });

    it("opens the sheet for a new kind", async () => {
      await userEvent.click(screen.getByRole("button", { name: "Column menu post_author" }));
      await userEvent.click(await screen.findByRole("menuitem", { name: "Add kind…" }));
      expect(await screen.findByRole("dialog", { name: "new kind" })).toBeInTheDocument();
    });

    it("opens the sheet for a new reference", async () => {
      await userEvent.click(screen.getByRole("button", { name: "Column menu post_author" }));
      await userEvent.click(await screen.findByRole("menuitem", { name: "Add reference…" }));
      expect(await screen.findByRole("dialog", { name: "new reference" })).toBeInTheDocument();
    });

    it("switches the tab to the rows", async () => {
      await userEvent.click(screen.getByRole("button", { name: "Column menu post_author" }));
      await userEvent.click(await screen.findByRole("menuitem", { name: "Open rows" }));
      expect(useStore.getState().tabs[0]).toMatchObject({ id: tab.id, view: "data" });
    });
  });
});
