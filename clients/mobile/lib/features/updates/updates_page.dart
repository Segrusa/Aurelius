import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import '../chat/chat_view.dart' show markdownStyle;
import '../markdown/links.dart';
import 'updates_bloc.dart';
import '../../core/widgets/ios.dart';

/// Settings → Updates: the server's version, and installing a new one from the phone.
class UpdatesPage extends StatelessWidget {
  const UpdatesPage({super.key});

  @override
  Widget build(BuildContext context) {
    return BlocProvider(
      create: (context) => UpdatesBloc(context.read<AureliusRepository>()),
      child: Scaffold(
        appBar: const GlassAppBar(
          title: Text('Updates'),
          actions: [
            InfoIcon(
              'Only a release signed with the Aurelius key is installed. Work on the PC continues after the restart. '
              'Apps on other computers update separately.',
              label: 'Updates',
            ),
          ],
        ),
        body: BlocConsumer<UpdatesBloc, UpdatesState>(
          listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
          listener: (context, state) => toast(context, state.notice!),
          builder: (context, state) {
            if (state.loading)
              return const Center(child: CupertinoActivityIndicator());
            final status = state.status;
            if (status == null)
              return ErrorRetry(
                state.error ?? 'No update information.',
                onRetry: () =>
                    context.read<UpdatesBloc>().add(const UpdatesRequested()),
              );
            final bloc = context.read<UpdatesBloc>();
            final notes = (status.notes ?? '').trim();
            return RefreshIndicator.adaptive(
              onRefresh: () async =>
                  bloc.add(const UpdatesRequested(check: true)),
              child: ListView(
                physics: const AlwaysScrollableScrollPhysics(
                  parent: BouncingScrollPhysics(),
                ),
                padding: EdgeInsets.fromLTRB(
                  0,
                  12,
                  0,
                  32 + MediaQuery.paddingOf(context).bottom,
                ),
                children: [
                  _Hero(status: status, summary: _summary(status)),
                  if (state.busy)
                    Padding(
                      padding: const EdgeInsets.only(top: 16),
                      child: InsetGroup(children: [_Progress(state: state)]),
                    )
                  else if (status.enabled && status.available)
                    Padding(
                      padding: const EdgeInsets.fromLTRB(16, 16, 16, 0),
                      child: FilledButton(
                        onPressed: () => _install(context, status),
                        child: Text('Install ${status.latest} and Restart'),
                      ),
                    ),
                  const GroupHeader('This PC', caps: true),
                  InsetGroup(
                    children: [
                      GroupRow(
                        title: 'Installed',
                        value: status.current,
                        chevron: false,
                      ),
                      GroupRow(
                        title: 'Latest',
                        value: status.latest ?? '—',
                        chevron: false,
                      ),
                      if (status.checkedAtMs > 0)
                        GroupRow(
                          title: 'Last checked',
                          value: Fmt.ago(status.checkedAtMs),
                          chevron: false,
                        ),
                      if (!state.busy)
                        InkWell(
                          onTap: status.enabled
                              ? () => bloc.add(
                                  const UpdatesRequested(check: true),
                                )
                              : null,
                          child: Padding(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 16,
                              vertical: 13,
                            ),
                            child: Text(
                              'Check Now',
                              style: TextStyle(
                                fontSize: 17,
                                color: status.enabled
                                    ? Tokens.accent
                                    : Tokens.label3,
                              ),
                            ),
                          ),
                        ),
                    ],
                  ),
                  const GroupHeader('Server', caps: true),
                  InsetGroup(
                    children: [
                      InkWell(
                        onTap: state.busy ? null : () => _restart(context),
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 16,
                            vertical: 13,
                          ),
                          child: Text(
                            'Restart Server',
                            style: TextStyle(
                              fontSize: 17,
                              color: state.busy
                                  ? Tokens.label3
                                  : Tokens.negative,
                            ),
                          ),
                        ),
                      ),
                    ],
                  ),
                  if (notes.isNotEmpty) ...[
                    GroupHeader(
                      status.latest == null
                          ? "What's New"
                          : "What's New in ${status.latest}",
                      caps: true,
                    ),
                    InsetGroup(
                      padding: const EdgeInsets.fromLTRB(16, 14, 16, 14),
                      children: [
                        ConstrainedBox(
                          constraints: const BoxConstraints(maxHeight: 280),
                          child: Scrollbar(
                            child: SingleChildScrollView(
                              child: MessageMarkdown(
                                data: notes,
                                scope: null,
                                selectable: true,
                                styleSheet: markdownStyle(context).copyWith(
                                  p: TextStyle(
                                    fontSize: 15,
                                    height: 1.45,
                                    color: Tokens.label,
                                  ),
                                  listBullet: TextStyle(
                                    fontSize: 15,
                                    height: 1.45,
                                    color: Tokens.label2,
                                  ),
                                  listIndent: 18,
                                  blockSpacing: 10,
                                ),
                              ),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ],
                ],
              ),
            );
          },
        ),
      ),
    );
  }

  String _summary(UpdateStatus s) {
    if (!s.enabled) return s.reason ?? 'Updates are off on this PC.';
    if (s.error != null) return s.error!;
    if (s.available) return 'Version ${s.latest} is available.';
    if (s.checkedAtMs > 0) return 'The PC runs the latest version.';
    return 'Not checked yet. Pull down or press Check now.';
  }

  Future<void> _install(BuildContext context, UpdateStatus status) async {
    final bloc = context.read<UpdatesBloc>();
    final ok = await confirm(
      context,
      title: 'Install Aurelius ${status.latest}?',
      body:
          'The PC downloads and verifies the release, then the background service restarts. '
          'Running work continues after the restart; this phone reconnects by itself.',
      action: 'Install',
    );
    if (ok) bloc.add(const UpdateInstallRequested());
  }

  Future<void> _restart(BuildContext context) async {
    final bloc = context.read<UpdatesBloc>();
    final ok = await confirm(
      context,
      title: 'Restart the server?',
      body:
          'The background service on the PC stops and starts again. '
          'Answers that are running now are interrupted; this phone reconnects by itself.',
      action: 'Restart',
      danger: true,
    );
    if (ok) bloc.add(const ServerRestartRequested());
  }
}

class _Progress extends StatelessWidget {
  const _Progress({required this.state});

  final UpdatesState state;

  @override
  Widget build(BuildContext context) {
    final text = switch (state.phase) {
      UpdatePhase.downloading =>
        'Downloading and verifying the release on the PC…',
      UpdatePhase.installing => 'Installing ${state.target ?? ''}…',
      UpdatePhase.restarting =>
        'The PC is restarting Aurelius. Waiting until it is back…',
      UpdatePhase.stopping => 'Asking the server to restart…',
      UpdatePhase.rebooting =>
        'The server is restarting. Waiting until it is back…',
      UpdatePhase.idle => '',
    };
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
      child: Row(
        children: [
          const CupertinoActivityIndicator(),
          const SizedBox(width: 12),
          Expanded(
            child: Text(
              text,
              style: TextStyle(fontSize: 15, color: Tokens.label),
            ),
          ),
        ],
      ),
    );
  }
}

/// The version at a glance: what runs, and whether something newer waits.
class _Hero extends StatelessWidget {
  const _Hero({required this.status, required this.summary});

  final UpdateStatus status;
  final String summary;

  @override
  Widget build(BuildContext context) {
    final (icon, color) = !status.enabled || status.error != null
        ? (CupertinoIcons.exclamationmark_circle_fill, Tokens.warning)
        : status.available
        ? (CupertinoIcons.arrow_down_circle_fill, Tokens.accent)
        : (CupertinoIcons.checkmark_seal_fill, Tokens.accent);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: SurfaceCard(
        padding: const EdgeInsets.fromLTRB(20, 24, 20, 22),
        child: Column(
          children: [
            Container(
              width: 64,
              height: 64,
              decoration: BoxDecoration(
                color: color.withValues(alpha: 0.16),
                borderRadius: BorderRadius.circular(18),
              ),
              child: Icon(icon, size: 34, color: color),
            ),
            const SizedBox(height: 14),
            Text('Aurelius ${status.current}', style: Tokens.title(size: 22)),
            const SizedBox(height: 4),
            Text(
              summary,
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 15,
                height: 1.35,
                color: Tokens.label2,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
