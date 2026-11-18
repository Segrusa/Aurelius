import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../hosts/hosts_bloc.dart';

/// Paired PCs: switch, pair another, or forget one on this phone.
class HostsPage extends StatelessWidget {
  const HostsPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: const GlassAppBar(title: Text('Paired PCs')),
      body: BlocBuilder<HostsBloc, HostsState>(
        builder: (context, state) => ListView(
          padding: EdgeInsets.only(top: 16, bottom: listBottom(context)),
          children: [
            InsetGroup(
              dividerIndent: 54,
              children: [
                for (final h in state.hosts)
                  GroupRow(
                    leading: Icon(
                      h.hostId == state.active?.hostId ? CupertinoIcons.checkmark_alt_circle_fill : CupertinoIcons.circle,
                      color: h.hostId == state.active?.hostId ? Tokens.accent : Tokens.label3,
                      size: 24,
                    ),
                    title: h.isDev ? '${h.name} (dev)' : h.name,
                    subtitle: 'Paired ${Fmt.ago(h.pairedAtMs)}',
                    onTap: h.hostId == state.active?.hostId ? null : () => context.read<HostsBloc>().add(HostSelected(h.hostId)),
                    trailing: IconButton(
                      tooltip: 'Forget',
                      icon: Icon(CupertinoIcons.trash, color: Tokens.red, size: 20),
                      onPressed: () async {
                        if (await confirm(
                          context,
                          title: 'Forget ${h.name}?',
                          body: 'This phone forgets the PC. To also remove the phone there, revoke it in Aurelius on the PC (Settings → Remote access).',
                          action: 'Forget',
                          danger: true,
                        )) {
                          if (context.mounted) context.read<HostsBloc>().add(HostForgotten(h.hostId));
                        }
                      },
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 24),
            InsetGroup(
              children: [
                InkWell(
                  onTap: () => context.push('/pair'),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 13),
                    child: Row(children: [
                      Icon(CupertinoIcons.add, color: Tokens.accent, size: 20),
                      const SizedBox(width: 10),
                      Text('Pair Another PC', style: TextStyle(fontSize: 17, color: Tokens.accent)),
                    ]),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
