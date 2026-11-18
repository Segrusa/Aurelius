import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/push/push_service.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart' show confirm;
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart' show Monogram, toast;
import '../../data/aurelius_repository.dart';
import '../account/claude_login.dart' show claudeLoginLine;
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import '../hosts/hosts_bloc.dart';
import '../server_name/server_name_bloc.dart';
import '../server_name/server_name_section.dart';

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

String _monogram(String name) => name.isEmpty ? '?' : String.fromCharCode(name.runes.first).toUpperCase();

/// The Profile tab (ticket #72), one page instead of More › Account and
/// More › Settings, in the style of iOS Settings: the signed-in board member
/// and their account (sessions, Claude login, password), the PC this phone
/// talks to, what is used while operating (employees, notifications, board,
/// board members, paired PCs), About and Sign out. The PC's own settings are
/// changed on the desktop.
class ProfilePage extends StatelessWidget {
  const ProfilePage({super.key});

  Future<void> _signOut(BuildContext context) async {
    final repository = context.read<AureliusRepository>();
    final ok = await confirm(context, title: 'Sign out on this phone?', body: 'You need your password to come back.', action: 'Sign out', danger: true);
    if (!ok) return;
    try {
      await repository.signOut();
    } catch (e) {
      if (context.mounted) toast(context, '$e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final repository = context.read<AureliusRepository>();
    final host = repository.host;
    final link = context.watch<ConnectionBloc>().state;
    final user = link.user;
    final push = context.read<PushService>();
    final paired = context.select<HostsBloc, int>((b) => b.state.hosts.length);
    final employees = context.select<CompanyBloc, int>((b) => b.state.activeEmployees.length);
    return LargeTitleScaffold(
      title: 'Profile',
      children: [
        if (user != null) ...[
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 12, 20, 8),
            child: Row(children: [
              Monogram(_monogram(user.username), active: true, size: 48),
              const SizedBox(width: 14),
              Expanded(child: Text(user.username, key: const ValueKey('profile-name'), style: Tokens.title(size: 22))),
            ]),
          ),
          const SizedBox(height: 12),
          const GroupHeader('Account'),
          InsetGroup(
            dividerIndent: 60,
            children: [
              GroupRow(
                key: const ValueKey('account-sessions'),
                leading: const IconTile(CupertinoIcons.device_phone_portrait),
                title: 'Sessions',
                chevron: true,
                onTap: () => context.push('/account/sessions'),
              ),
              GroupRow(
                key: const ValueKey('account-claude'),
                leading: const IconTile(CupertinoIcons.sparkles),
                title: 'Claude login',
                value: claudeLoginLine(link.claudeLogin),
                chevron: true,
                onTap: () => context.push('/account/claude'),
              ),
              GroupRow(
                key: const ValueKey('account-password'),
                leading: const IconTile(CupertinoIcons.lock_fill),
                title: 'Change password',
                chevron: true,
                onTap: () => context.push('/account/password'),
              ),
            ],
          ),
          const SizedBox(height: 8),
        ],
        const GroupHeader('Settings'),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 0),
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
        // Lazy: a server without the feature is never asked its name (SN-02 AC4).
        BlocProvider(create: (_) => ServerNameBloc(repository)..add(const ServerNameRefreshed()), child: ServerNameSection(link: link)),
        const SizedBox(height: 20),
        InsetGroup(
          dividerIndent: 60,
          children: [
            // The way to the chat with an employee, and to Give work from an employee's page.
            GroupRow(
              key: const ValueKey('profile-employees'),
              leading: const IconTile(CupertinoIcons.person_2_fill),
              title: 'Employees',
              value: employees == 0 ? null : '$employees',
              chevron: true,
              onTap: () => context.push('/company'),
            ),
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
            if (link.features.contains('employee_run_overview'))
              GroupRow(key: const ValueKey('settings-employee-runs'), leading: const IconTile(CupertinoIcons.slider_horizontal_3), title: 'Model & effort', subtitle: 'Per employee', chevron: true, onTap: () => context.push('/settings/employee-runs')),
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
        const SizedBox(height: 20),
        InsetGroup(
          dividerIndent: 60,
          children: [
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
        if (user != null) ...[
          const SizedBox(height: 20),
          InsetGroup(children: [
            GroupRow(
              key: const ValueKey('account-sign-out'),
              title: 'Sign out',
              titleStyle: TextStyle(fontSize: 17, color: Tokens.negative),
              chevron: false,
              onTap: () => _signOut(context),
            ),
          ]),
        ],
        const SizedBox(height: 32),
      ],
    );
  }
}
