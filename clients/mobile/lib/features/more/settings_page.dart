import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/push/push_service.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart' show InfoIcon;
import '../../data/aurelius_repository.dart';
import '../../data/run_config.dart';
import '../../data/views.dart';
import '../artifacts/artifacts.dart';
import '../connection/connection_bloc.dart';
import '../chat/chat_view.dart' show showModelSheet;
import '../hosts/hosts_bloc.dart';
import '../load/load_widgets.dart';
import '../power/power_widgets.dart';
import '../runtime/runtime_bloc.dart';
import 'app_keys_section.dart';
import 'auto_allow_section.dart';
import 'mcp_section.dart';

/// The app's version as pubspec.yaml has it ("0.8.3+2"), read from the copy
/// bundled as an asset; "unknown" when it cannot be read.
Future<String> appVersion() async {
  try {
    final pubspec = await rootBundle.loadString('pubspec.yaml');
    return RegExp(r'^version:\s*(\S+)', multiLine: true).firstMatch(pubspec)?.group(1) ?? 'unknown';
  } catch (_) {
    return 'unknown';
  }
}

/// "Opus 5.5 · High", or null before the runtime is known.
String? _modelValue(RunChoice? choice) {
  final model = choice?.model;
  if (model == null) return null;
  return [ModelCatalog.friendly(model), if (choice!.effort != null) ModelCatalog.effortLabel(choice.effort)].join(' · ');
}

/// Settings, in the style of iOS Settings: the PC this phone talks to, then
/// what is used while operating (models, notifications, updates, usage,
/// paired PCs). The PC's own settings are under "Advanced", mostly
/// read-only: they are changed on the desktop.
class SettingsPage extends StatelessWidget {
  const SettingsPage({super.key});

  @override
  Widget build(BuildContext context) {
    final repository = context.read<AureliusRepository>();
    final host = repository.host;
    final link = context.watch<ConnectionBloc>().state;
    final push = context.read<PushService>();
    final paired = context.select<HostsBloc, int>((b) => b.state.hosts.length);
    return LargeTitleScaffold(
      title: 'Settings',
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 20, 16, 0),
          child: Material(
            color: Tokens.cell,
            clipBehavior: Clip.antiAlias,
            shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Tokens.cardRadius)),
            child: InkWell(
              onTap: () => context.push('/hosts'),
              child: Padding(
                padding: const EdgeInsets.all(14),
                child: Row(
                  children: [
                    Container(
                      width: 52,
                      height: 52,
                      decoration: BoxDecoration(color: Tokens.accentTint, borderRadius: BorderRadius.circular(14)),
                      child: Icon(CupertinoIcons.desktopcomputer, size: 28, color: Tokens.accent),
                    ),
                    const SizedBox(width: 14),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(children: [
                            Flexible(child: Text(host.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: Tokens.title(size: 20).copyWith(fontWeight: FontWeight.w600))),
                            if (host.isDev) ...[
                              const SizedBox(width: 8),
                              Text('DEV', style: TextStyle(color: Tokens.devBadge, fontWeight: FontWeight.w700, fontSize: 12)),
                            ],
                          ]),
                          const SizedBox(height: 2),
                          Row(children: [
                            Container(width: 7, height: 7, decoration: BoxDecoration(shape: BoxShape.circle, color: link.isOnline ? Tokens.accent : Tokens.warning)),
                            const SizedBox(width: 6),
                            Expanded(
                              child: Text(
                                link.isOnline ? 'Connected${link.daemonVersion != null ? ' · Aurelius ${link.daemonVersion}' : ''}' : (link.message ?? 'Not connected'),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(color: Tokens.label2, fontSize: 15),
                              ),
                            ),
                          ]),
                          const SizedBox(height: 3),
                          Text(host.url, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(color: Tokens.label3, fontSize: 12)),
                        ],
                      ),
                    ),
                    const Chevron(),
                  ],
                ),
              ),
            ),
          ),
        ),
        const GroupHeader('Models', caps: true),
        BlocBuilder<RuntimeBloc, RuntimeState>(
          builder: (context, runtime) => InsetGroup(
            dividerIndent: 60,
            children: [
              GroupRow(
                key: ValueKey('settings-model-${Purpose.employee.wire}'),
                leading: const IconTile(CupertinoIcons.person_2_fill),
                title: 'Employees',
                value: _modelValue(runtime.choice(Purpose.employee)),
                chevron: true,
                onTap: () => showModelSheet(context, Purpose.employee),
              ),
            ],
          ),
        ),
        const SizedBox(height: 20),
        InsetGroup(
          dividerIndent: 60,
          children: [
            ValueListenableBuilder<PushState>(
              valueListenable: push.state,
              builder: (context, state, _) => GroupRow(
                padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
                leading: const IconTile(CupertinoIcons.bell_fill),
                title: 'Notifications',
                subtitle: switch (state) {
                  PushState.unsupported => 'Not available on this phone yet.',
                  PushState.denied => 'Turned off in the phone\'s settings.',
                  PushState.failed => push.lastError ?? 'Could not turn on notifications.',
                  PushState.registered || PushState.off => null,
                },
                trailing: CupertinoSwitch(
                  value: state == PushState.registered,
                  activeTrackColor: Tokens.accent,
                  onChanged: state == PushState.unsupported ? null : (on) => on ? push.enable(repository) : push.disable(repository),
                ),
              ),
            ),
            GroupRow(key: const ValueKey('settings-board'), leading: const IconTile(CupertinoIcons.square_split_2x1), title: 'Board', subtitle: 'Columns and dispatcher', chevron: true, onTap: () => context.push('/settings/board')),
            GroupRow(leading: const IconTile(CupertinoIcons.arrow_down_to_line), title: 'Updates', chevron: true, onTap: () => context.push('/updates')),
            GroupRow(leading: const IconTile(CupertinoIcons.chart_bar_alt_fill), title: 'Usage', chevron: true, onTap: () => context.push('/usage')),
            GroupRow(
              key: const ValueKey('settings-board-members'),
              leading: const IconTile(CupertinoIcons.person_crop_circle_fill),
              title: 'Board members',
              value: link.user?.username,
              chevron: true,
              onTap: () => context.push('/settings/board-members'),
            ),
            GroupRow(leading: const IconTile(CupertinoIcons.device_laptop), title: 'Paired PCs', value: '$paired', chevron: true, onTap: () => context.push('/hosts')),
          ],
        ),
        PowerSection(link: link, serverName: host.name),
        const SizedBox(height: 20),
        InsetGroup(
          dividerIndent: 60,
          children: [
            GroupRow(
              key: const ValueKey('settings-advanced'),
              leading: const IconTile(CupertinoIcons.slider_horizontal_3),
              title: 'Advanced',
              trailing: const InfoIcon('Plugins, load, MCP & tools, app keys, recurring tasks.', label: 'Advanced'),
              chevron: true,
              onTap: () => context.push('/settings/advanced'),
            ),
            GroupRow(
              leading: const IconTile(CupertinoIcons.info),
              title: 'About Aurelius',
              onTap: () async {
                final version = await appVersion();
                if (!context.mounted) return;
                showAboutDialog(
                  context: context,
                  applicationName: 'Aurelius',
                  applicationVersion: version,
                  children: const [
                    Text('The PC stays the host: projects, memory, credentials and runtimes never leave it. '
                        'This phone gives work, follows it up, chats and approves; the PC\'s settings are changed in Aurelius on the desktop.'),
                  ],
                );
              },
            ),
          ],
        ),
        const SizedBox(height: 32),
      ],
    );
  }
}

/// Settings → Advanced: what the PC itself is set up with, mostly to look at
/// (it is changed on the desktop): plugins, the load limits, MCP servers and
/// tools, app keys, the claude.ai account and the recurring tasks.
class AdvancedPage extends StatelessWidget {
  const AdvancedPage({super.key});

  @override
  Widget build(BuildContext context) {
    final link = context.watch<ConnectionBloc>().state;
    return LargeTitleScaffold(
      title: 'Advanced',
      children: [
        const SizedBox(height: 20),
        InsetGroup(
          dividerIndent: 60,
          children: [
            GroupRow(key: const ValueKey('advanced-plugins'), leading: const IconTile(Icons.extension), title: 'Plugins', chevron: true, onTap: () => context.push('/plugins')),
            GroupRow(
              key: const ValueKey('advanced-recurring'),
              leading: const IconTile(CupertinoIcons.repeat),
              title: 'Recurring tasks',
              chevron: true,
              onTap: () => context.push('/settings/recurring'),
            ),
          ],
        ),
        LoadLimitsSection(link: link),
        const McpSection(),
        const McpAlwaysAllowSection(),
        const ArtifactsStatus(),
        const AutoAllowSection(),
        const AppKeysSection(),
        const ClaudeAiAccountSection(),
        const SizedBox(height: 32),
      ],
    );
  }
}
