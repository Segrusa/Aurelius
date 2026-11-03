import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../core/usage/screen_usage.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';

/// Settings → Screen use: how often each screen was opened on this phone.
/// Kept on this device only; nothing is sent to the PC.
class ScreenUsePage extends StatelessWidget {
  const ScreenUsePage({super.key, required this.usage});

  final ScreenUsage usage;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: usage,
      builder: (context, _) {
        final rows = usage.counter.ranked();
        return LargeTitleScaffold(
          title: 'Screen use (this device only)',
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
              child: Text(
                'How often each screen was opened on this phone. Nothing leaves the phone.',
                style: TextStyle(color: Tokens.label2, fontSize: 13),
              ),
            ),
            if (rows.isEmpty)
              Padding(
                padding: const EdgeInsets.all(24),
                child: Center(
                  child: Text(
                    'No screens counted yet.',
                    style: TextStyle(color: Tokens.label2),
                  ),
                ),
              )
            else
              InsetGroup(
                children: [
                  for (final (i, e) in rows.indexed)
                    GroupRow(
                      key: ValueKey('screen-use-$i'),
                      title: e.key,
                      subtitle: 'Last opened ${Fmt.ago(e.value.lastMs)}',
                      value: '${e.value.opens}',
                    ),
                ],
              ),
            const SizedBox(height: 20),
            InsetGroup(
              children: [
                GroupRow(
                  key: const ValueKey('screen-use-reset'),
                  title: 'Reset counts',
                  titleStyle: TextStyle(color: Tokens.red, fontSize: 17),
                  onTap: rows.isEmpty
                      ? null
                      : () async {
                          final ok = await confirm(
                            context,
                            title: 'Reset counts?',
                            body: 'Every screen goes back to zero.',
                            action: 'Reset',
                            danger: true,
                          );
                          if (ok) await usage.reset();
                        },
                ),
              ],
            ),
          ],
        );
      },
    );
  }
}
