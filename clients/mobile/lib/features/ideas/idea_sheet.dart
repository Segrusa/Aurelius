import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../data/views.dart';
import 'idea_page.dart';
import 'ideas_bloc.dart';

/// An idea in full (its note with its line breaks, its files with thumbnails)
/// and what can be done with it. Pops with the chosen action: `start:<id>:<name>`,
/// `task`, `edit`, `delete` or a status. Follows the [IdeasBloc], so an edit
/// elsewhere shows at once and a deleted idea closes the sheet.
class IdeaSheet extends StatelessWidget {
  const IdeaSheet({super.key, required this.idea, required this.employees, this.canDelete = false});

  final IdeaView idea;
  final List<EmployeeSummary> employees;

  /// The server knows `idea.delete` (feature `idea_attachments`).
  final bool canDelete;

  @override
  Widget build(BuildContext context) {
    return BlocConsumer<IdeasBloc, IdeasState>(
      listenWhen: (a, b) => !b.loading && a.ideas.any((i) => i.id == idea.id) && !b.ideas.any((i) => i.id == idea.id),
      listener: (context, _) => Navigator.maybePop(context),
      builder: (context, state) {
        final idea = state.ideas.where((i) => i.id == this.idea.id).firstOrNull ?? this.idea;
        final note = idea.note;
        return SafeArea(
          child: ConstrainedBox(
            constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.85),
            child: ListView(
              shrinkWrap: true,
              children: [
                Padding(padding: const EdgeInsets.fromLTRB(16, 16, 16, 4), child: Text(idea.title, key: const Key('idea-sheet-title'), style: Tokens.title(size: 18))),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  child: Text(
                    [idea.kind == IdeaKind.bug ? 'Bug' : 'Feature', idea.byEmployee ? 'from ${idea.author}' : 'from you'].join(' · '),
                    style: TextStyle(color: Tokens.muted, fontSize: 13),
                  ),
                ),
                if (note != null && note.trim().isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
                    child: SelectableText(note, key: const Key('idea-sheet-note'), style: const TextStyle(fontSize: 15, height: 1.35)),
                  ),
                if (idea.attachments.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 14, 16, 0),
                    child: Wrap(
                      spacing: 8,
                      runSpacing: 8,
                      crossAxisAlignment: WrapCrossAlignment.end,
                      children: [for (final f in idea.attachments) IdeaAttachment(file: f)],
                    ),
                  ),
                const SizedBox(height: 8),
                const Divider(height: 1),
                if (idea.taskId == null)
                  for (final m in employees)
                    ListTile(
                      leading: CircleAvatar(child: Text(m.monogram)),
                      title: Text('Start as a task for ${m.name}'),
                      subtitle: Text(m.roleLine),
                      onTap: () => Navigator.pop(context, 'start:${m.id}:${m.name}'),
                    ),
                if (idea.taskId != null) ListTile(leading: const Icon(Icons.task_alt), title: const Text('Open the task'), onTap: () => Navigator.pop(context, 'task')),
                ListTile(leading: const Icon(Icons.edit_outlined), title: const Text('Edit'), onTap: () => Navigator.pop(context, 'edit')),
                if (idea.status != IdeaStatus.open) ListTile(leading: const Icon(Icons.undo), title: const Text('Reopen'), onTap: () => Navigator.pop(context, 'open')),
                if (idea.status != IdeaStatus.done) ListTile(leading: const Icon(Icons.check), title: const Text('Mark done'), onTap: () => Navigator.pop(context, 'done')),
                if (idea.status != IdeaStatus.dropped)
                  ListTile(leading: Icon(Icons.close, color: Tokens.negative), title: const Text('Drop'), onTap: () => Navigator.pop(context, 'dropped')),
                if (canDelete)
                  ListTile(
                    key: const Key('idea-delete'),
                    leading: Icon(Icons.delete_outline, color: Tokens.negative),
                    title: Text('Delete', style: TextStyle(color: Tokens.negative)),
                    onTap: () => Navigator.pop(context, 'delete'),
                  ),
              ],
            ),
          ),
        );
      },
    );
  }
}
