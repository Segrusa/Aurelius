import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/widgets.dart';
import '../../data/line_diff.dart';
import '../../data/views.dart';
import 'diff_view.dart';

/// New instructions an employee proposed for itself or a direct report
/// (feature `instruction_proposals`): the diff current ↔ proposed, the
/// evidence, and Reject, Edit (approve an edited text) or Approve. When the
/// instructions changed after the proposal ([ProposalState.isStale])
/// approving is refused, so only Reject is offered.
class InstructionProposalCard extends StatelessWidget {
  const InstructionProposalCard({super.key, required this.proposal, required this.onDecide});

  final ProposalView proposal;

  /// [instructions]: the edited text when approved with edits.
  final void Function(bool approve, String? note, String? instructions) onDecide;

  @override
  Widget build(BuildContext context) {
    final p = proposal;
    final target = p.targetEmployeeName ?? 'an employee';
    final self = p.targetEmployeeId != null && p.targetEmployeeId == p.proposedBy;
    return SurfaceCard(
      key: ValueKey('proposal-${p.id}'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('New instructions', style: TextStyle(fontSize: 13, color: Tokens.label2)),
          const SizedBox(height: 4),
          Text(target, style: Tokens.title(size: 18)),
          Text(self ? 'Proposed by $target for itself' : 'Proposed by ${p.proposedByName}', style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
          if (p.reason.isNotEmpty) ...[
            const SizedBox(height: 8),
            Text(p.reason, style: const TextStyle(fontSize: 14)),
          ],
          if (p.evidence.isNotEmpty) ...[
            const SizedBox(height: 8),
            Wrap(
              spacing: 6,
              runSpacing: 4,
              children: [
                for (final e in p.evidence)
                  Chip(
                    visualDensity: VisualDensity.compact,
                    avatar: Icon(e.kind == 'task' ? Icons.task_alt : Icons.lightbulb_outline, size: 15),
                    label: Text(e.label, overflow: TextOverflow.ellipsis),
                  ),
              ],
            ),
          ],
          if (p.isStale)
            Padding(
              key: ValueKey('proposal-stale-${p.id}'),
              padding: const EdgeInsets.only(top: 10),
              child: Text(
                "$target's instructions changed after this proposal (version ${p.currentVersion} now, ${p.baseVersion} then). Approving would overwrite that change; reject it and the employee can propose again.",
                style: TextStyle(color: Tokens.warning, fontSize: 12.5),
              ),
            ),
          const SizedBox(height: 10),
          DiffLines(key: ValueKey('proposal-diff-${p.id}'), lines: lineDiff(p.currentInstructions ?? '', p.instructions)),
          const SizedBox(height: 10),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            children: [
              TextButton(
                key: ValueKey('decline-${p.id}'),
                onPressed: () async {
                  final note = await promptForm(context, title: 'Reject these instructions?', action: 'Reject', fields: const [FieldSpec('Note for the employee', required: false, lines: 2)]);
                  if (note == null) return;
                  onDecide(false, note.first.isEmpty ? null : note.first, null);
                },
                child: const Text('Reject'),
              ),
              if (!p.isStale) ...[
                const SizedBox(width: 4),
                TextButton(
                  key: ValueKey('edit-${p.id}'),
                  onPressed: () async {
                    final edited = await EditInstructionsPage.open(context, employeeName: target, initial: p.instructions);
                    if (edited == null || edited.trim().isEmpty) return;
                    onDecide(true, null, edited == p.instructions ? null : edited);
                  },
                  child: const Text('Edit'),
                ),
                const SizedBox(width: 4),
                FilledButton(
                  key: ValueKey('approve-${p.id}'),
                  onPressed: () => onDecide(true, null, null),
                  child: const Text('Approve'),
                ),
              ],
            ],
          ),
        ],
      ),
    );
  }
}

/// The proposed instructions in a full-screen editor; "Approve" returns the text.
class EditInstructionsPage extends StatefulWidget {
  const EditInstructionsPage({super.key, required this.employeeName, required this.initial});

  final String employeeName;
  final String initial;

  static Future<String?> open(BuildContext context, {required String employeeName, required String initial}) => Navigator.of(context).push<String>(
        MaterialPageRoute(fullscreenDialog: true, builder: (_) => EditInstructionsPage(employeeName: employeeName, initial: initial)),
      );

  @override
  State<EditInstructionsPage> createState() => _EditInstructionsPageState();
}

class _EditInstructionsPageState extends State<EditInstructionsPage> {
  late final _text = TextEditingController(text: widget.initial);

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Scaffold(
        appBar: AppBar(
          title: Text('Instructions for ${widget.employeeName}'),
          leading: IconButton(tooltip: 'Cancel', icon: const Icon(Icons.close), onPressed: () => Navigator.pop(context)),
          actions: [
            Padding(
              padding: const EdgeInsets.only(right: 12),
              child: FilledButton(
                key: const Key('approve-edited-instructions'),
                onPressed: _text.text.trim().isEmpty ? null : () => Navigator.pop(context, _text.text),
                child: const Text('Approve'),
              ),
            ),
          ],
        ),
        body: SafeArea(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: TextField(
              key: const Key('edit-instructions-field'),
              controller: _text,
              autofocus: true,
              expands: true,
              maxLines: null,
              textAlignVertical: TextAlignVertical.top,
              keyboardType: TextInputType.multiline,
              decoration: const InputDecoration(alignLabelWithHint: true, labelText: 'Instructions'),
              onChanged: (_) => setState(() {}),
            ),
          ),
        ),
      );
}
