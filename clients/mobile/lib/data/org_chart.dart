import 'protocol/protocol.dart';

/// One employee in the org chart and how far down it sits (0 = reports to nobody).
class OrgNode {
  const OrgNode(this.employee, this.depth);

  final EmployeeSummary employee;
  final int depth;
}

/// The employees as a tree in reading order: each employee is followed by those who
/// report to it, by name. An employee whose boss is not in [employees] (archived)
/// counts as a root, as on the server.
List<OrgNode> orgChart(List<EmployeeSummary> employees) {
  final ids = {for (final m in employees) m.id};
  final byBoss = <String?, List<EmployeeSummary>>{};
  for (final m in employees) {
    final boss = ids.contains(m.reportsTo) && m.reportsTo != m.id ? m.reportsTo : null;
    byBoss.putIfAbsent(boss, () => []).add(m);
  }
  for (final list in byBoss.values) {
    list.sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
  }
  final out = <OrgNode>[];
  final seen = <String>{};
  void walk(EmployeeSummary m, int depth) {
    if (!seen.add(m.id)) return; // a cycle the server would have refused
    out.add(OrgNode(m, depth));
    for (final report in byBoss[m.id] ?? const <EmployeeSummary>[]) {
      walk(report, depth + 1);
    }
  }

  for (final root in byBoss[null] ?? const <EmployeeSummary>[]) {
    walk(root, 0);
  }
  return out;
}

/// The employees who can be someone's boss: everyone but itself and, to avoid a
/// cycle the server would refuse, those who report to it (directly or not).
List<EmployeeSummary> possibleBosses(List<EmployeeSummary> employees, EmployeeSummary self) {
  final below = <String>{};
  void collect(String id) {
    for (final m in employees) {
      if (m.reportsTo == id && below.add(m.id)) collect(m.id);
    }
  }

  collect(self.id);
  return [for (final m in employees) if (m.id != self.id && !below.contains(m.id)) m];
}
