import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import 'ideas_bloc.dart';
import 'ideas_tab.dart';

/// Ideas (under More): the ideas of every project in one list, each labelled with its project, with a
/// project filter (default: all projects). A new idea asks for its project, starting with the
/// last one used on this phone.
class IdeasPage extends StatefulWidget {
  const IdeasPage({super.key});

  @override
  State<IdeasPage> createState() => _IdeasPageState();
}

class _IdeasPageState extends State<IdeasPage> {
  /// The project the last new idea went to.
  static const _pref = 'ideas.project';

  Future<String?> _chooseProject(BuildContext context, List<ProjectSummary> projects, {required String title, String? checked, bool allowAll = false}) {
    return showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 16, 16, 4), child: Text(title, style: TextStyle(fontSize: 13, color: Tokens.label2))),
            if (allowAll)
              ListTile(
                key: const ValueKey('ideas-all-projects'),
                title: const Text('All projects'),
                trailing: checked == null ? const Icon(Icons.check) : null,
                onTap: () => Navigator.pop(sheet, ''),
              ),
            for (final p in projects)
              ListTile(
                title: Text(p.name),
                trailing: p.id == checked ? const Icon(Icons.check) : null,
                onTap: () => Navigator.pop(sheet, p.id),
              ),
          ],
        ),
      ),
    );
  }

  Future<void> _filter(BuildContext context, List<ProjectSummary> projects, String? current) async {
    final picked = await _chooseProject(context, projects, title: 'Show ideas of', checked: current, allowAll: true);
    if (picked == null || !context.mounted) return;
    context.read<IdeasBloc>().add(IdeasProjectChanged(picked.isEmpty ? null : picked));
  }

  Future<void> _add(BuildContext context, List<ProjectSummary> projects, String? filtered) async {
    final prefs = await SharedPreferences.getInstance();
    final known = projects.map((p) => p.id).toSet();
    final saved = prefs.getString(_pref);
    // The filtered project, else the last one used, else the first.
    final suggested = [filtered, saved].where((id) => id != null && known.contains(id)).firstOrNull ?? projects.first.id;
    if (!context.mounted) return;
    final project = projects.length == 1 ? projects.single.id : await _chooseProject(context, projects, title: 'Add the idea to', checked: suggested);
    if (project == null || project.isEmpty || !context.mounted) return;
    await prefs.setString(_pref, project);
    if (!context.mounted) return;
    await addIdea(context, projectId: project);
  }

  @override
  Widget build(BuildContext context) {
    final company = context.watch<CompanyBloc>().state;
    final projects = company.visibleProjects;
    final top = MediaQuery.paddingOf(context).top;
    return Scaffold(
      backgroundColor: Tokens.bg,
      body: projects.isEmpty
          ? Column(children: [
              SizedBox(height: top + 16),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 20),
                child: Row(children: [
                  if (ModalRoute.of(context)?.canPop ?? false) ...[
                    GlassButton(tooltip: 'Back', icon: CupertinoIcons.chevron_back, onPressed: () => Navigator.of(context).maybePop()),
                    const SizedBox(width: 10),
                  ],
                  Text('Ideas', style: Tokens.title(size: 34)),
                ]),
              ),
              const Expanded(child: EmptyState('No projects yet', detail: 'Ideas belong to a project. Make one in Projects first.')),
            ])
          : BlocProvider(
              create: (context) => IdeasBloc(context.read<AureliusRepository>()),
              child: Builder(
                builder: (context) {
                  final filtered = context.select((IdeasBloc b) => b.state.project);
                  final name = projects.where((p) => p.id == filtered).firstOrNull?.name ?? 'All projects';
                  return Column(
                    children: [
                      SizedBox(height: top + 8),
                      Padding(
                        padding: const EdgeInsets.fromLTRB(20, 0, 16, 0),
                        child: Row(
                          children: [
                            if (ModalRoute.of(context)?.canPop ?? false) ...[
                              GlassButton(tooltip: 'Back', icon: CupertinoIcons.chevron_back, onPressed: () => Navigator.of(context).maybePop()),
                              const SizedBox(width: 10),
                            ],
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text('Ideas', style: Tokens.title(size: 34)),
                                  Text(name, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 15, color: Tokens.label2)),
                                ],
                              ),
                            ),
                            GlassButton(tooltip: 'Filter by project', icon: CupertinoIcons.folder, onPressed: () => _filter(context, projects, filtered)),
                            const SizedBox(width: 10),
                            GlassButton(tooltip: 'New idea', icon: CupertinoIcons.add, onPressed: () => _add(context, projects, filtered)),
                          ],
                        ),
                      ),
                      const Expanded(child: IdeasTab()),
                    ],
                  );
                },
              ),
            ),
    );
  }
}
