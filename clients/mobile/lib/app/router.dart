import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../core/hosts/host.dart';
import '../core/net/gateway_api.dart';
import '../core/push/push_action.dart';
import '../core/widgets/widgets.dart' show toast;
import '../data/aurelius_repository.dart';
import '../features/ideas/ideas_page.dart';
import '../features/company/projects_page.dart';
import '../features/company_org/company_org_page.dart';
import '../features/company_org/department_page.dart';
import '../features/company_org/employee_page.dart';
import '../features/dashboard/dashboard_page.dart';
import '../features/more/more_page.dart';
import '../features/more/recurring_page.dart';
import '../features/tasks/tasks_page.dart';
import '../features/team/team_page.dart';
import '../features/inbox/approval_page.dart';
import '../features/inbox/inbox_page.dart';
import '../features/more/hosts_page.dart';
import '../features/more/settings_page.dart';
import '../features/pairing/pair_page.dart';
import '../features/pairing/pairing_bloc.dart';
import '../features/project/project_page.dart';
import '../features/project/task_page.dart';
import '../features/usage/context_page.dart';
import '../features/plugins/plugins_page.dart';
import '../features/updates/updates_page.dart';
import '../features/usage/usage_page.dart';
import 'shell.dart';

/// Routes for one host. Detail screens cover the tab bar (root navigator).
class AppRouter {
  AppRouter(this.host);

  final Host host;
  final _root = GlobalKey<NavigatorState>(debugLabel: 'root');

  late final GoRouter config = GoRouter(
    navigatorKey: _root,
    initialLocation: '/home',
    routes: [
      StatefulShellRoute.indexedStack(
        builder: (context, state, shell) => HomeShell(shell: shell),
        branches: [
          StatefulShellBranch(routes: [
            GoRoute(path: '/home', pageBuilder: (context, state) => routePage(state, const DashboardPage())),
          ]),
          StatefulShellBranch(routes: [
            GoRoute(path: '/team', pageBuilder: (context, state) => routePage(state, const TeamPage())),
          ]),
          StatefulShellBranch(routes: [
            GoRoute(path: '/inbox', pageBuilder: (context, state) => routePage(state, const InboxPage())),
          ]),
          StatefulShellBranch(routes: [
            GoRoute(path: '/tasks', pageBuilder: (context, state) => routePage(state, const TasksPage())),
          ]),
          StatefulShellBranch(routes: [
            GoRoute(path: '/more', pageBuilder: (context, state) => routePage(state, const MorePage())),
          ]),
        ],
      ),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/project/:pid',
        pageBuilder: (context, state) => routePage(state, ProjectPage(projectId: state.pathParameters['pid']!, initialTab: state.uri.queryParameters['tab'])),
        routes: [
          GoRoute(
            parentNavigatorKey: _root,
            path: 'task/:tid',
            pageBuilder: (context, state) => routePage(state, TaskPage(projectId: state.pathParameters['pid']!, taskId: state.pathParameters['tid']!)),
          ),
        ],
      ),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/employee/:eid',
        pageBuilder: (context, state) => routePage(state, EmployeePage(employeeId: state.pathParameters['eid']!)),
        routes: [
          GoRoute(
            parentNavigatorKey: _root,
            path: 'tasks',
            pageBuilder: (context, state) => routePage(state, TasksPage(employeeId: state.pathParameters['eid']!)),
          ),
        ],
      ),
      // A task conversation (writable) or an earlier chat of an employee.
      GoRoute(
        parentNavigatorKey: _root,
        path: '/conversation/:cid',
        pageBuilder: (context, state) => routePage(state, ConversationPage(conversationId: state.pathParameters['cid']!, employeeId: state.uri.queryParameters['e'])),
      ),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/company',
        pageBuilder: (context, state) => routePage(state, CompanyOrgPage(initial: state.uri.queryParameters['view'])),
      ),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/department/:did',
        pageBuilder: (context, state) => routePage(state, DepartmentPage(departmentId: state.pathParameters['did']!)),
      ),
      GoRoute(parentNavigatorKey: _root, path: '/projects', pageBuilder: (context, state) => routePage(state, const ProjectsPage())),
      GoRoute(parentNavigatorKey: _root, path: '/ideas', pageBuilder: (context, state) => routePage(state, const IdeasPage())),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/settings',
        pageBuilder: (context, state) => routePage(state, const SettingsPage()),
        routes: [
          GoRoute(parentNavigatorKey: _root, path: 'advanced', pageBuilder: (context, state) => routePage(state, const AdvancedPage())),
          GoRoute(parentNavigatorKey: _root, path: 'recurring', pageBuilder: (context, state) => routePage(state, const RecurringPage())),
        ],
      ),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/approval/:aid',
        pageBuilder: (context, state) => routePage(state, ApprovalPage(approvalId: state.pathParameters['aid']!)),
      ),
      GoRoute(parentNavigatorKey: _root, path: '/usage', pageBuilder: (context, state) => routePage(state, const UsagePage())),
      GoRoute(parentNavigatorKey: _root, path: '/plugins', pageBuilder: (context, state) => routePage(state, const PluginsPage())),
      GoRoute(parentNavigatorKey: _root, path: '/updates', pageBuilder: (context, state) => routePage(state, const UpdatesPage())),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/context/:runId',
        pageBuilder: (context, state) => routePage(state, ContextPage(runId: state.pathParameters['runId']!)),
      ),
      GoRoute(parentNavigatorKey: _root, path: '/hosts', pageBuilder: (context, state) => routePage(state, const HostsPage())),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/pair',
        pageBuilder: (context, state) => routePage(state, BlocProvider(
          create: (context) => PairingBloc(context.read<GatewayApi>(), platform: Platform.isIOS ? 'ios' : 'android'),
          child: const PairPage(firstRun: false),
        )),
      ),
    ],
  );

  /// Open what a push notification points at (routing data only).
  void openNotification(Map<String, dynamic> data, AureliusRepository repository) {
    final target = data['target_type'] as String? ?? '';
    final taskId = data['task_id'] as String?;
    final projectId = data['project_id'] as String?;
    final aggregateId = data['aggregate_id'] as String?;
    if (target == 'approval.requested' && aggregateId != null) {
      // Notification action "Goedkeuren": decide right away, open the approval
      // when that does not work. "Bekijk" and a plain tap just open it.
      if (data['action'] == 'approve') {
        unawaited(_approveFromPush(aggregateId, repository));
        return;
      }
      _openApproval(aggregateId);
    } else if (taskId != null && projectId != null) {
      config.go('/tasks');
      config.push('/project/$projectId/task/$taskId');
    } else {
      config.go('/inbox');
    }
  }

  void _openApproval(String approvalId) {
    config.go('/inbox');
    config.push('/approval/$approvalId');
  }

  Future<void> _approveFromPush(String approvalId, AureliusRepository repository) async {
    final result = await approveFromPush(repository, approvalId);
    if (!result.done) _openApproval(approvalId);
    final context = _root.currentContext;
    if (context != null && context.mounted) toast(context, result.message);
  }

  void dispose() => config.dispose();
}

/// Flutter's own MaterialPage, so the theme's page transitions apply: the iOS
/// edge swipe back and the slide. go_router 18 looks for the MaterialApp of
/// the material_ui package; ours is Flutter's, so it would fall back to plain
/// pages without a transition or back gesture.
Page<void> routePage(GoRouterState state, Widget child) =>
    MaterialPage<void>(key: state.pageKey, name: state.name ?? state.path, restorationId: state.pageKey.value, child: child);
