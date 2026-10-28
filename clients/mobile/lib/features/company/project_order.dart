import 'package:shared_preferences/shared_preferences.dart';

import '../../data/views.dart';

/// The user's own order of the projects list, kept on this phone per host.
/// The server has no project order; desktop only pins tabs.
class ProjectOrderStore {
  const ProjectOrderStore(this.hostId);

  final String hostId;

  String get _key => 'projects.order.$hostId';

  Future<List<String>> load() async {
    try {
      return (await SharedPreferences.getInstance()).getStringList(_key) ?? const [];
    } catch (_) {
      // No preferences (tests, a broken store): the server's order.
      return const [];
    }
  }

  Future<void> save(List<String> ids) async {
    try {
      await (await SharedPreferences.getInstance()).setStringList(_key, ids);
    } catch (_) {
      // Not saved; the order still holds until the app restarts.
    }
  }
}

/// [projects] in the saved [order]. Projects the order does not know yet
/// (new, or made on another device) follow in the server's order.
List<ProjectSummary> inProjectOrder(List<ProjectSummary> projects, List<String> order) {
  if (order.isEmpty) return projects;
  final rank = {for (final (i, id) in order.indexed) id: i};
  final known = projects.where((p) => rank.containsKey(p.id)).toList()..sort((a, b) => rank[a.id]!.compareTo(rank[b.id]!));
  return [...known, ...projects.where((p) => !rank.containsKey(p.id))];
}

/// The ids of [projects] after dragging the row at [from] so it ends at [to].
List<String> movedProject(List<ProjectSummary> projects, int from, int to) {
  final ids = projects.map((p) => p.id).toList();
  if (from < 0 || from >= ids.length) return ids;
  final id = ids.removeAt(from);
  ids.insert(to.clamp(0, ids.length), id);
  return ids;
}
