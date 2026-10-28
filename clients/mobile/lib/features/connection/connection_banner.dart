import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/net/connection.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import 'connection_bloc.dart';

/// Shown whenever the link is not plainly online: which PC, what is wrong,
/// and how old the data on screen is.
class ConnectionBanner extends StatelessWidget {
  const ConnectionBanner({super.key});

  /// When shown, the banner takes the status-bar inset from the page below.
  static bool visible(LinkState link) => !(link.isOnline && link.message == null);

  @override
  Widget build(BuildContext context) {
    final link = context.watch<ConnectionBloc>().state;
    final host = context.read<AureliusRepository>().host;
    final top = MediaQuery.paddingOf(context).top;
    if (!ConnectionBanner.visible(link)) return const SizedBox.shrink();

    final (color, icon, title) = switch (link.status) {
      LinkStatus.online => (Tokens.warning, Icons.info_outline, 'Read-only'),
      LinkStatus.connecting => (Tokens.text2, Icons.sync, 'Connecting to ${host.name}…'),
      LinkStatus.offline => (Tokens.warning, Icons.cloud_off_outlined, '${host.name} is offline'),
      LinkStatus.revoked => (Tokens.negative, Icons.block, 'Removed on ${host.name}'),
      LinkStatus.incompatible => (Tokens.negative, Icons.error_outline, link.message == differentVersions ? 'Update needed' : 'Cannot use ${host.name}'),
    };
    final stale = link.lastOnlineMs == null ? 'No data yet.' : 'Showing data from ${Fmt.ago(link.lastOnlineMs!)}.';
    return Material(
      color: Tokens.surface,
      child: Container(
        padding: EdgeInsets.fromLTRB(16, top + 8, 8, 8),
        decoration: BoxDecoration(border: Border(bottom: BorderSide(color: Tokens.border))),
        child: Row(
          children: [
            Icon(icon, size: 18, color: color),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(title, style: TextStyle(color: color, fontWeight: FontWeight.w600, fontSize: 13.5)),
                  Text(
                    [if (link.message != null) link.message!, if (!link.isOnline) stale].join(' '),
                    style: TextStyle(color: Tokens.text2, fontSize: 12.5),
                  ),
                ],
              ),
            ),
            if (link.status == LinkStatus.offline || link.status == LinkStatus.connecting)
              TextButton(onPressed: () => context.read<ConnectionBloc>().add(const ConnectionRetryRequested()), child: const Text('Retry')),
            if (link.status == LinkStatus.revoked || (link.status == LinkStatus.incompatible && link.message != differentVersions))
              TextButton(onPressed: () => context.push('/hosts'), child: const Text('Pair again')),
          ],
        ),
      ),
    );
  }
}
