import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (orig) => ({ ...(await orig<object>()), api: (await import("../test/apiMock")).createApiMock() }));

import { api } from "../api";
import { connectedStore } from "../test/fixtures";
import { rulesInfoOf, showTableIcons } from "../test/iconFixtures";
import { IconGlyph, TableIcon } from "./TableIcon";

const mocked = vi.mocked(api, true);
const iconOf = (container: HTMLElement) => container.querySelector("[data-icon]")?.getAttribute("data-icon") ?? null;

describe("TableIcon", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    connectedStore();
  });

  it("draws the icon the database's rules give the table, hidden from assistive technology", () => {
    showTableIcons({ wp_users: "users" });
    const { container } = render(<TableIcon database="db" table="wp_users" />);
    expect(container.querySelector("[data-icon=users]")).toHaveAttribute("aria-hidden", "true");
  });

  it("draws the default table icon for a table without a rule icon", () => {
    showTableIcons({ wp_users: "users" });
    const { container } = render(<TableIcon database="db" table="wp_posts" fallback={<span>plain</span>} />);
    expect(iconOf(container)).toBe("table");
    expect(screen.queryByText("plain")).not.toBeInTheDocument();
  });

  it("draws the fallback without a table", () => {
    showTableIcons({ wp_users: "users" });
    const { container } = render(<TableIcon database="db" table={null} fallback={<span>plain</span>} />);
    expect(screen.getByText("plain")).toBeInTheDocument();
    expect(iconOf(container)).toBeNull();
  });

  it("draws nothing without a table", () => {
    showTableIcons({ wp_users: "users" });
    const { container } = render(<TableIcon database="db" table={null} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("draws the default table icon and loads nothing without a database", () => {
    const { container } = render(<TableIcon database={undefined} table="wp_users" />);
    expect(iconOf(container)).toBe("table");
    expect(mocked.rulesInfo).not.toHaveBeenCalled();
  });

  it("draws the default table icon before the rules load, then the rule's icon", async () => {
    mocked.rulesInfo.mockResolvedValue(rulesInfoOf({ wp_users: "users" }));
    const { container } = render(<TableIcon database="db" table="wp_users" />);
    expect(iconOf(container)).toBe("table");
    await waitFor(() => expect(iconOf(container)).toBe("users"));
  });

  it("shows the click indicator in the icon's place when given an effect", () => {
    showTableIcons({ wp_users: "users" });
    const { container } = render(<TableIcon database="db" table="wp_users" effect="drill" />);
    const swap = container.querySelector("[data-open=drill]")!;
    expect(swap).toHaveAttribute("aria-hidden", "true");
    expect(swap.querySelector("[data-icon=users]")).not.toBeNull();
    expect(swap.querySelector(".open-glyph")).not.toBeNull();
  });

  it("draws the bare icon without an effect", () => {
    showTableIcons({ wp_users: "users" });
    const { container } = render(<TableIcon database="db" table="wp_users" />);
    expect(container.querySelector("[data-open]")).toBeNull();
  });

  it("loads the database's icons once for many icons and fills them in", async () => {
    mocked.rulesInfo.mockResolvedValue(rulesInfoOf({ wp_users: "user", wp_posts: "file-text" }));
    const { container } = render(
      <>
        <TableIcon database="db" table="wp_users" />
        <TableIcon database="db" table="wp_posts" />
        <TableIcon database="db" table="wp_users" />
      </>,
    );
    await waitFor(() =>
      expect([...container.querySelectorAll("[data-icon]")].map((el) => el.getAttribute("data-icon"))).toEqual(["user", "file-text", "user"]),
    );
    expect(mocked.rulesInfo).toHaveBeenCalledOnce();
    expect(mocked.rulesInfo).toHaveBeenCalledWith("s1", "db");
  });

  it("does not load again when the icons are already known", () => {
    showTableIcons({});
    render(<TableIcon database="db" table="wp_users" />);
    expect(mocked.rulesInfo).not.toHaveBeenCalled();
  });

  it("loads the icons of the given session instead of the active one", async () => {
    mocked.rulesInfo.mockResolvedValue(rulesInfoOf({ wp_users: "user" }));
    render(<TableIcon database="other" table="wp_users" sessionId="s2" />);
    await waitFor(() => expect(mocked.rulesInfo).toHaveBeenCalledWith("s2", "other"));
  });
});

describe("IconGlyph", () => {
  it("draws nothing without a name", () => {
    const { container } = render(<IconGlyph name={null} />);
    expect(iconOf(container)).toBeNull();
  });
});
