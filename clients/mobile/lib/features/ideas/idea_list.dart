import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../core/widgets/ios.dart';
import '../../data/views.dart';

/// Ideas in two sections, "From you" first and "From employees" below (an empty
/// section is left out), so who noted an idea is clear at a glance.
class IdeaList extends StatelessWidget {
  const IdeaList({super.key, required this.ideas, required this.onTap, required this.onDrop, this.projectName});

  final List<IdeaView> ideas;

  /// The project's name shown on each idea (the company-wide list); null leaves it out.
  final String Function(IdeaView)? projectName;
  final void Function(IdeaView) onTap;
  final void Function(IdeaView) onDrop;

  @override
  Widget build(BuildContext context) {
    final yours = ideas.where((i) => !i.byEmployee).toList();
    final employees = ideas.where((i) => i.byEmployee).toList();
    return ListView(
      padding: EdgeInsets.only(bottom: listBottom(context)),
      children: [
        if (yours.isNotEmpty) ..._section('From you', yours, key: 'ideas-yours'),
        if (employees.isNotEmpty) ..._section('From employees', employees, key: 'ideas-employees'),
      ],
    );
  }

  List<Widget> _section(String title, List<IdeaView> ideas, {required String key}) => [
        Eyebrow('$title · ${ideas.length}', key: ValueKey(key), padding: const EdgeInsets.fromLTRB(16, 18, 16, 6)),
        for (final (i, idea) in ideas.indexed) ...[
          if (i > 0) const Divider(height: 1, indent: 16),
          _IdeaRow(idea: idea, project: projectName?.call(idea), onTap: () => onTap(idea), onDrop: () => onDrop(idea)),
        ],
      ];
}

class _IdeaRow extends StatelessWidget {
  const _IdeaRow({required this.idea, required this.project, required this.onTap, required this.onDrop});

  final IdeaView idea;
  final String? project;
  final VoidCallback onTap;
  final VoidCallback onDrop;

  @override
  Widget build(BuildContext context) {
    final byEmployee = idea.byEmployee;
    final tile = ListTile(
      leading: CircleAvatar(
        radius: 14,
        backgroundColor: byEmployee ? Tokens.surface2 : Tokens.accentTint,
        child: byEmployee
            ? Text(idea.author.characters.first.toUpperCase(), style: const TextStyle(fontSize: 12))
            : Icon(Icons.person_outline, size: 16, color: Tokens.accent),
      ),
      title: Row(children: [
        Expanded(child: Text(idea.title)),
        const SizedBox(width: 8),
        _KindBadge(idea.kind),
      ]),
      subtitle: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            byEmployee ? 'Idea from ${idea.author}' : 'You',
            style: TextStyle(color: byEmployee ? Tokens.managerMark : Tokens.label2, fontSize: 13, fontWeight: byEmployee ? FontWeight.w600 : null),
          ),
          if (project != null)
            Text(project!, key: ValueKey('idea-project-${idea.id}'), maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: Tokens.label2, fontSize: 13)),
          if (idea.taskId != null || idea.note != null || idea.attachments.isNotEmpty)
            Text(
              [
                if (idea.taskId != null) 'task ${idea.taskStatus?.name ?? ''}'.trim(),
                if (idea.attachments.isNotEmpty) idea.attachments.length == 1 ? '1 file' : '${idea.attachments.length} files',
                if (idea.note != null) idea.note!,
              ].join(' · '),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(color: Tokens.muted, fontSize: 13),
            ),
        ],
      ),
      trailing: const Icon(Icons.more_horiz),
      onTap: onTap,
    );
    return Dismissible(
      key: ValueKey('idea-${idea.id}'),
      direction: idea.status == IdeaStatus.open ? DismissDirection.endToStart : DismissDirection.none,
      background: Container(
        alignment: Alignment.centerRight,
        padding: const EdgeInsets.only(right: 20),
        color: Tokens.surface2,
        child: Text('Drop', style: TextStyle(color: Tokens.negative)),
      ),
      onDismissed: (_) => onDrop(),
      child: byEmployee
          ? Container(
              key: ValueKey('idea-accent-${idea.id}'),
              decoration: BoxDecoration(border: Border(left: BorderSide(color: Tokens.managerMark, width: 4))),
              child: tile,
            )
          : tile,
    );
  }
}

/// Feature/Bug tag; Bug stands out in red.
class _KindBadge extends StatelessWidget {
  const _KindBadge(this.kind);

  final IdeaKind kind;

  @override
  Widget build(BuildContext context) {
    final bug = kind == IdeaKind.bug;
    final color = bug ? Tokens.negative : Tokens.label2;
    return Container(
      key: ValueKey('idea-kind-${kind.name}'),
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(color: color.withValues(alpha: 0.14), borderRadius: BorderRadius.circular(6)),
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        Icon(bug ? Icons.bug_report_outlined : Icons.lightbulb_outline, size: 12, color: color),
        const SizedBox(width: 3),
        Text(bug ? 'Bug' : 'Feature', style: TextStyle(fontSize: 11, fontWeight: FontWeight.w600, color: color)),
      ]),
    );
  }
}
