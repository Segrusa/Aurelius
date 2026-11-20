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

/// The Profile tab: who you are and where you are signed in, then the parts
/// with the desktop's words (Settings › You): Account (Claude login, where
/// you are signed in, password), Workspace (employees, projects, board
/// members, model & effort, board, paired PCs), App (notifications, about)
/// and Sign out on its own. The PC's own settings are changed on the desktop.
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

  Future<void> _about(BuildContext context) async {
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
  }

  @override
  Widget build(BuildContext context) {
    final repository = context.read<AureliusRepository>();
    final link = context.watch<ConnectionBloc>().state;
    final user = link.user;
    final push = context.read<PushService>();
    final paired = context.select<HostsBloc, int>((b) => b.state.hosts.length);
    final employees = context.select<CompanyBloc, int>((b) => b.state.activeEmployees.length);
    final projects = context.select<CompanyBloc, int>((b) => b.state.openProjects.length);
    return LargeTitleScaffold(
      title: 'Profile',
      children: [
        _Hero(username: user?.username),
        // The PC's name, with the PC above. Lazy: a server without the feature is never asked its name (SN-02 AC4).
        BlocProvider(create: (_) => ServerNameBloc(repository)..add(const ServerNameRefreshed()), child: ServerNameSection(link: link)),
        if (user != null) ...[
          const GroupHeader('Account'),
          InsetGroup(
            dividerIndent: 60,
            children: [
              GroupRow(
                key: const ValueKey('account-claude'),
                leading: const IconTile(CupertinoIcons.sparkles),
                title: 'Claude login',
                value: claudeLoginLine(link.claudeLogin),
                chevron: true,
                onTap: () => context.push('/account/claude'),
              ),
              GroupRow(
                key: const ValueKey('account-sessions'),
                leading: const IconTile(CupertinoIcons.device_phone_portrait),
                title: 'Signed in on',
                chevron: true,
                onTap: () => context.push('/account/sessions'),
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
        ],
        const SizedBox(height: 8),
        const GroupHeader('Workspace'),
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
            GroupRow(
              key: const ValueKey('profile-projects'),
              leading: const IconTile(CupertinoIcons.folder_fill),
              title: 'Projects',
              value: projects == 0 ? null : '$projects',
              chevron: true,
              onTap: () => context.push('/projects'),
            ),
            GroupRow(
              key: const ValueKey('settings-board-members'),
              leading: const IconTile(CupertinoIcons.person_crop_circle_fill),
              title: 'Board members',
              chevron: true,
              onTap: () => context.push('/settings/board-members'),
            ),
            if (link.features.contains('employee_run_overview'))
              GroupRow(key: const ValueKey('settings-employee-runs'), leading: const IconTile(CupertinoIcons.slider_horizontal_3), title: 'Model & effort', subtitle: 'Per employee', chevron: true, onTap: () => context.push('/settings/employee-runs')),
            GroupRow(key: const ValueKey('settings-board'), leading: const IconTile(CupertinoIcons.square_split_2x1), title: 'Board', subtitle: 'Columns and dispatcher', chevron: true, onTap: () => context.push('/settings/board')),
            GroupRow(key: const ValueKey('profile-paired'), leading: const IconTile(CupertinoIcons.device_laptop), title: 'Paired PCs', value: '$paired', chevron: true, onTap: () => context.push('/hosts')),
          ],
        ),
        const SizedBox(height: 8),
        const GroupHeader('App'),
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
            GroupRow(key: const ValueKey('profile-about'), leading: const IconTile(CupertinoIcons.info), title: 'About Aurelius', chevron: true, onTap: () => _about(context)),
          ],
        ),
        if (user != null) ...[
          const SizedBox(height: 28),
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

/// The top of Profile: your monogram and name, and the PC this phone talks to
/// with whether it is connected; a tap opens the PCs.
class _Hero extends StatelessWidget {
  const _Hero({required this.username});

  /// Null while nobody is signed in: only the PC.
  final String? username;

  @override
  Widget build(BuildContext context) {
    final host = context.read<AureliusRepository>().host;
    final link = context.watch<ConnectionBloc>().state;
    final name = username;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
      child: Material(
        color: Tokens.cell,
        clipBehavior: Clip.antiAlias,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(Tokens.cardRadius)),
        child: Column(children: [
          if (name != null)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 20, 16, 14),
              child: Column(children: [
                Monogram(_monogram(name), active: true, size: 72),
                const SizedBox(height: 10),
                Text(name, key: const ValueKey('profile-name'), style: Tokens.title(size: 24)),
                const SizedBox(height: 2),
                Text('Board member', style: TextStyle(color: Tokens.label2, fontSize: 14)),
              ]),
            ),
          if (name != null) Divider(height: 1, thickness: 0.5, color: Tokens.sep, indent: 16, endIndent: 16),
          InkWell(
            key: const ValueKey('profile-pc'),
            onTap: () => context.push('/hosts'),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(16, 12, 12, 12),
              child: Row(children: [
                Icon(CupertinoIcons.desktopcomputer, size: 22, color: Tokens.accent),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Row(children: [
                      Flexible(child: Text(host.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600))),
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
                          style: TextStyle(color: Tokens.label2, fontSize: 13),
                        ),
                      ),
                    ]),
                  ]),
                ),
                const Chevron(),
              ]),
            ),
          ),
        ]),
      ),
    );
  }
}
