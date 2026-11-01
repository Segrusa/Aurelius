import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../data/aurelius_repository.dart';
import '../../data/guidelines.dart';
import '../../data/views.dart';
import 'guideline_common.dart';
import 'guideline_page.dart';

/// "Guidelines: Rust v3, Git & worktrees v4" in a run's details (P-59, B6):
/// the versions the run had in its system text, from `run.guidelines`; a tap
/// on one opens exactly that version (GL-11 AC3). Nothing on a server without
/// guidelines or for a run without them.
class RunGuidelinesLine extends StatefulWidget {
  const RunGuidelinesLine({super.key, required this.runId});

  final String runId;

  @override
  State<RunGuidelinesLine> createState() => _RunGuidelinesLineState();
}

class _RunGuidelinesLineState extends State<RunGuidelinesLine> {
  Future<List<RunGuideline>>? _list;

  @override
  void initState() {
    super.initState();
    final repo = context.read<AureliusRepository>();
    if (guidelinesOn(context)) _list = repo.runGuidelineList(widget.runId);
  }

  @override
  Widget build(BuildContext context) => FutureBuilder<List<RunGuideline>>(
        future: _list,
        builder: (context, snap) {
          final list = snap.data;
          final line = list == null ? null : runGuidelinesLine(list);
          if (line == null) return const SizedBox.shrink();
          return Padding(
            key: const ValueKey('run-guidelines'),
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(line, style: TextStyle(color: Tokens.text2)),
              const SizedBox(height: 4),
              Wrap(spacing: 6, runSpacing: 4, children: [
                for (final g in list!)
                  ActionChip(
                    key: ValueKey('run-guideline-${g.guidelineId}'),
                    visualDensity: VisualDensity.compact,
                    label: Text('${g.title} v${g.version}'),
                    tooltip: g.level == GuidelineLevel.project ? 'From the project' : 'Linked to the employee',
                    onPressed: () => openGuideline(context, g.guidelineId, version: g.version),
                  ),
              ]),
            ]),
          );
        },
      );
}
