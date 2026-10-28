import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart' show InfoIcon;
import '../../data/views.dart';
import '../project/variants_picker.dart';

/// The mode a started idea's task runs in; `mode == null` is "Employee's default".
/// `variants`: try it that many ways first (1 = as usual).
typedef StartMode = ({AgentMode? mode, int variants});

/// Asks in which mode the task for [employeeName] starts. Null when dismissed.
/// Without [canAuto] (no Approve right on this phone) Auto is shown but off.
/// [maxVariants] above 1 (feature `task_variants`, switched on) adds the
/// variants picker above the modes.
Future<StartMode?> pickStartMode(BuildContext context, {required String employeeName, required bool canAuto, int maxVariants = 1}) {
  var variants = 1;
  Widget option(BuildContext sheet, AgentMode? mode, String title, String detail, {bool enabled = true}) => ListTile(
        key: Key('start-mode-${mode?.wire ?? 'default'}'),
        enabled: enabled,
        leading: Icon(mode == null ? Icons.radio_button_checked : Icons.radio_button_unchecked, color: mode == AgentMode.auto && enabled ? Tokens.warning : null),
        title: Text(title),
        subtitle: enabled ? null : Text(detail),
        trailing: enabled ? InfoIcon(detail, label: title) : null,
        onTap: () => Navigator.pop(sheet, (mode: mode, variants: variants)),
      );

  return showModalBottomSheet<StartMode>(
    context: context,
    isScrollControlled: true,
    builder: (sheet) => StatefulBuilder(
      builder: (sheet, setState) => SafeArea(
      child: ListView(
        shrinkWrap: true,
        children: [
          if (maxVariants > 1)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: variantsPicker(sheet, max: maxVariants, value: variants, onChanged: (n) => setState(() => variants = n)),
              ),
            ),
          Padding(padding: const EdgeInsets.fromLTRB(16, 16, 16, 8), child: Text('Start for $employeeName in', style: Tokens.title(size: 18))),
          option(sheet, null, "Employee's default", "The mode $employeeName works in."),
          option(sheet, AgentMode.ask, AgentMode.ask.label, 'Ask before every change outside the chat.'),
          option(sheet, AgentMode.acceptEdits, AgentMode.acceptEdits.label, 'Change files in the project folder without asking.'),
          option(sheet, AgentMode.plan, AgentMode.plan.label, 'Only read and plan; change nothing.'),
          option(
            sheet,
            AgentMode.auto,
            AgentMode.auto.label,
            canAuto ? 'Do everything that would ask. Deny rules still apply.' : 'Needs the Approve right; pair this phone with it to start in Auto.',
            enabled: canAuto,
          ),
        ],
      ),
    ),
    ),
  );
}
