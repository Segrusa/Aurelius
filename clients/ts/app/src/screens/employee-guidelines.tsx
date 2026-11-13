// The Guidelines tab of the employee page: which guidelines of the library its runs get, linked
// and unlinked here, and how full its block is.

import { CheckboxInput } from "@astryxdesign/core/CheckboxInput";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import { Table, TableBody, TableCell, TableHeader, TableHeaderCell, TableRow } from "@astryxdesign/core/Table";
import { ProtocolError } from "@aurelius/client";
import * as stylex from "@stylexjs/stylex";
import { useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { useConnection, useLinkState } from "../connection";
import { useEffectiveSets, useEmployees, useGuidelines, useProjects } from "../data";
import { toast } from "../ui/toast";
import { count, failureText, READ_ONLY } from "./guideline-logic";
import { failureOf, Hint, Loading } from "./org-kit";

const styles = stylex.create({
  column: { display: "flex", flexDirection: "column", gap: 12 },
  table: { fontSize: 12 },
  headCell: { fontSize: 12, fontWeight: 400, color: "var(--precision-text3)" },
  title: { color: "var(--color-text-primary)", textDecoration: "none" },
  quiet: { color: "var(--color-text-secondary)" },
  links: { display: "flex", gap: 16, fontSize: 12 },
  manage: { color: "var(--color-text-accent)", textDecoration: "none" },
});

export function EmployeeGuidelinesTab({ employeeId, name }: { employeeId: string; name: string }) {
  const connection = useConnection();
  const client = useQueryClient();
  const { user } = useLinkState();
  const library = useGuidelines();
  const employees = useEmployees();
  const projects = useProjects();
  const sets = useEffectiveSets([[employeeId, null]]);
  const set = sets.data?.get(`${employeeId}|`);
  const [busy, setBusy] = useState<string | null>(null);
  const writable = user !== null;

  const toggle = async (guidelineId: string, link: boolean) => {
    setBusy(guidelineId);
    try {
      await connection.write(
        link ? "guideline.link" : "guideline.unlink",
        { guideline_id: guidelineId, target: { kind: "employee", id: employeeId } },
        { retry: false },
      );
      await client.invalidateQueries({ queryKey: ["guidelines"] });
      toast(`${link ? "Linked" : "Unlinked"}. It applies from ${name}'s next run.`);
    } catch (e) {
      const names = { employees: employees.data ?? [], projects: projects.data ?? [] };
      toast(
        e instanceof ProtocolError
          ? failureText({ code: e.code, message: e.message, tooLarge: e.tooLarge, matchedItems: e.matchedItems }, names)
          : failureOf(e),
      );
    } finally {
      setBusy(null);
    }
  };

  if (library.isPending) return <Loading rows={3} />;
  const list = [...(library.data ?? [])].sort((a, b) => a.title.localeCompare(b.title));
  const linked = (id: string) =>
    list.find((g) => g.id === id)?.links.some((l) => l.kind === "employee" && l.id === employeeId) ?? false;
  const over = set ? set.total_chars > set.limit_chars : false;

  return (
    <div {...stylex.props(styles.column)}>
      <Hint>
        {set
          ? `${name}'s runs get ${set.guidelines.length} guideline${set.guidelines.length === 1 ? "" : "s"} · ${count(set.total_chars)} / ${count(set.limit_chars)} characters. Guidelines of a project come on top in that project.`
          : `The guidelines ${name}'s runs get. Guidelines of a project come on top in that project.`}
      </Hint>
      {over ? <Hint>Over the limit: a run leaves guidelines out until the rest fits.</Hint> : null}
      {writable ? null : <Hint>{READ_ONLY}</Hint>}
      {list.length === 0 ? (
        <EmptyState title="No guidelines yet." description="Write the first one under Knowledge › Guidelines." />
      ) : (
        <Table aria-label={`Guidelines for ${name}`} xstyle={styles.table}>
          <TableHeader>
            <TableRow>
              <TableHeaderCell xstyle={styles.headCell}>Linked</TableHeaderCell>
              <TableHeaderCell xstyle={styles.headCell}>Guideline</TableHeaderCell>
              <TableHeaderCell xstyle={styles.headCell}>Characters</TableHeaderCell>
              <TableHeaderCell xstyle={styles.headCell}>Also for</TableHeaderCell>
            </TableRow>
          </TableHeader>
          <TableBody>
            {list.map((g) => {
              const on = linked(g.id);
              const others = g.links.flatMap((l) => {
                if (l.kind === "employee") {
                  if (l.id === employeeId) return [];
                  return [employees.data?.find((e) => e.id === l.id)?.name ?? "an archived employee"];
                }
                return [`project ${projects.data?.find((p) => p.id === l.id)?.name ?? "?"}`];
              });
              return (
                <TableRow key={g.id}>
                  <TableCell>
                    <CheckboxInput
                      label={`${on ? "Unlink" : "Link"} ${g.title}`}
                      isLabelHidden
                      value={on}
                      isDisabled={!writable || busy !== null}
                      onChange={() => void toggle(g.id, !on)}
                    />
                  </TableCell>
                  <TableCell>
                    <Link to="/knowledge/guidelines/$id" params={{ id: g.id }} {...stylex.props(styles.title)}>
                      {g.title}
                    </Link>
                  </TableCell>
                  <TableCell xstyle={styles.quiet}>{count(g.chars)}</TableCell>
                  <TableCell xstyle={styles.quiet}>{others.join(", ")}</TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      )}
      <span {...stylex.props(styles.links)}>
        <Link to="/knowledge/guidelines/$id" params={{ id: "new" }} {...stylex.props(styles.manage)}>
          New guideline
        </Link>
        <Link to="/knowledge/$tab" params={{ tab: "guidelines" }} {...stylex.props(styles.manage)}>
          All guidelines →
        </Link>
      </span>
    </div>
  );
}
