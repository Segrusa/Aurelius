import 'protocol/protocol.dart';

// The company seen from the phone: who sits at the top (the management team
// the Board gives work to), the order pickers list employees in, and who
// works in which project. Derived from employees and tasks only.

/// Employees that are not archived.
List<EmployeeSummary> activeEmployees(List<EmployeeSummary> employees) => employees.where((e) => e.status != LifecycleStatus.archived).toList();

/// The management team: active employees who report to nobody (or to someone
/// no longer in the company), by name.
List<EmployeeSummary> managementTeam(List<EmployeeSummary> employees) {
  final active = activeEmployees(employees);
  final ids = {for (final e in active) e.id};
  return active.where((e) => e.reportsTo == null || e.reportsTo == e.id || !ids.contains(e.reportsTo)).toList()..sort(byName);
}

/// Whether the server lets this employee choose Auto or Plan itself: the
/// management team and those who report to it directly.
bool isManagement(List<EmployeeSummary> employees, String id) {
  final team = {for (final e in managementTeam(employees)) e.id};
  if (team.contains(id)) return true;
  final active = {for (final e in activeEmployees(employees)) e.id};
  return employees.any((e) => e.id == id && active.contains(e.id) && team.contains(e.reportsTo));
}

int byName(EmployeeSummary a, EmployeeSummary b) => a.name.toLowerCase().compareTo(b.name.toLowerCase());

/// Who new work goes to unless the user picks someone else: the single root
/// of the company. With several roots (or none) nobody is preselected, so the
/// Board chooses.
String? defaultAssignee(List<EmployeeSummary> employees) {
  final roots = managementTeam(employees).where((e) => e.status == LifecycleStatus.active).toList();
  return roots.length == 1 ? roots.single.id : null;
}

/// One group of a picker: a heading and its employees.
typedef EmployeeGroup = ({String title, List<EmployeeSummary> employees});

/// Employees for "Who leads this task?": the management team first, then the
/// others by department (alphabetical, "No department" last), by name inside.
List<EmployeeGroup> pickerGroups(List<EmployeeSummary> employees, List<DepartmentSummary> departments) {
  final team = managementTeam(employees);
  final teamIds = {for (final e in team) e.id};
  final rest = activeEmployees(employees).where((e) => !teamIds.contains(e.id)).toList()..sort(byName);
  final names = {for (final d in departments) d.id: d.name};
  final byDepartment = <String?, List<EmployeeSummary>>{};
  for (final e in rest) {
    byDepartment.putIfAbsent(names.containsKey(e.departmentId) ? e.departmentId : null, () => []).add(e);
  }
  final keys = byDepartment.keys.whereType<String>().toList()..sort((a, b) => names[a]!.toLowerCase().compareTo(names[b]!.toLowerCase()));
  return [
    if (team.isNotEmpty) (title: 'Management team', employees: team),
    for (final k in keys) (title: names[k]!, employees: byDepartment[k]!),
    if (byDepartment[null] case final none?) (title: keys.isEmpty && team.isEmpty ? 'Employees' : 'No department', employees: none),
  ];
}

/// The employees of a department, its head first, then by name.
List<EmployeeSummary> departmentMembers(List<EmployeeSummary> employees, DepartmentSummary department) {
  final members = activeEmployees(employees).where((e) => e.departmentId == department.id).toList()..sort(byName);
  final head = members.where((e) => e.id == department.headId).toList();
  return [...head, ...members.where((e) => e.id != department.headId)];
}

/// Open tasks: not ended yet.
bool _open(TaskView t) => t.status != TaskStatus.succeeded && t.status != TaskStatus.failed && t.status != TaskStatus.cancelled;

/// Who works in a project: employees with an open task there, the busiest first.
List<EmployeeSummary> workingIn(String projectId, List<EmployeeSummary> employees, List<TaskView> tasks) {
  final counts = <String, int>{};
  for (final t in tasks) {
    if (t.projectId == projectId && _open(t) && t.employeeId != null) counts.update(t.employeeId!, (n) => n + 1, ifAbsent: () => 1);
  }
  final out = employees.where((e) => counts.containsKey(e.id)).toList()
    ..sort((a, b) {
      final c = counts[b.id]!.compareTo(counts[a.id]!);
      return c != 0 ? c : byName(a, b);
    });
  return out;
}

/// The projects an employee has open tasks in (ids, most recent first).
List<String> projectsOf(String employeeId, List<TaskView> tasks) {
  final open = tasks.where((t) => t.employeeId == employeeId && _open(t)).toList()..sort((a, b) => b.updatedAtMs.compareTo(a.updatedAtMs));
  return {for (final t in open) t.projectId}.toList();
}
