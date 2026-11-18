import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../core/hosts/host.dart';
import '../core/net/connection.dart' show BoardMemberStep;
import '../core/net/gateway_api.dart';
import '../core/push/push_action.dart';
import '../core/widgets/widgets.dart' show toast;
import '../data/aurelius_repository.dart';
import '../features/employee_runs/employee_runs_page.dart';
import '../features/account/account.dart';
import '../features/account/claude_login.dart' show ClaudeLoginPage;
import '../features/board/board_page.dart';
import '../features/board/board_settings_page.dart';
import '../data/discussion.dart';
import '../data/links.dart' show messageLocateFeature;
import '../features/board/new_ticket_page.dart';
import '../features/board/ticket_page.dart';
import '../features/company/projects_page.dart';
import '../features/company_org/company_org_page.dart';
import '../features/company_org/employee_page.dart';
import '../features/more/more_page.dart';
import '../features/links/link_actions.dart' show openAureliusLink, openMessageAt;
import '../features/tasks/tasks_page.dart';
import '../features/team/channel_link_page.dart';
import '../features/inbox/approval_page.dart';
import '../features/inbox/inbox_page.dart';
import '../features/inbox/inbox_rows.dart' show pushPlace;
import '../features/more/hosts_page.dart';
import '../features/more/settings_page.dart';
import '../features/onboarding/onboarding.dart';
import '../features/pairing/pair_page.dart';
import '../features/pairing/pairing_bloc.dart';
import '../features/project/project_page.dart';
import '../features/project/task_page.dart';
import '../features/sessions/new_session_page.dart';
import '../features/sessions/session_page.dart';
import '../features/sessions/sessions_page.dart' show WorkSessionsPage;
import 'shell.dart';

/// Routes for one host. Detail screens cover the tab bar (root navigator).
/// [boardMember]: the onboarding gate; without an active board member only
/// `/onboarding` and the server choice are reachable ([boardMemberRedirect]).
class AppRouter {
  AppRouter(this.host, {this.boardMember});

  final Host host;

  final ValueListenable<BoardMemberStep>? boardMember;
  final _root = GlobalKey<NavigatorState>(debugLabel: 'root');

  late final GoRouter config = GoRouter(
    navigatorKey: _root,
    initialLocation: '/sessions',
    refreshListenable: boardMember,
    redirect: (context, state) => boardMember == null ? null : boardMemberRedirect(boardMember!.value, state.matchedLocation),
    routes: [
      GoRoute(
        parentNavigatorKey: _root,
        path: '/onboarding',
        pageBuilder: (context, state) => routePage(state, const OnboardingPage()),
        routes: [
          GoRoute(parentNavigatorKey: _root, path: 'new', pageBuilder: (context, state) => routePage(state, const NewBoardMemberPage())),
        ],
      ),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/account',
        pageBuilder: (context, state) => routePage(state, const AccountPage()),
        routes: [
          GoRoute(parentNavigatorKey: _root, path: 'sessions', pageBuilder: (context, state) => routePage(state, const SessionsPage())),
          GoRoute(parentNavigatorKey: _root, path: 'password', pageBuilder: (context, state) => routePage(state, const ChangePasswordPage())),
          GoRoute(parentNavigatorKey: _root, path: 'claude', pageBuilder: (context, state) => routePage(state, const ClaudeLoginPage())),
        ],
      ),
      StatefulShellRoute.indexedStack(
        builder: (context, state, shell) => HomeShell(shell: shell),
        branches: [
          StatefulShellBranch(routes: [
            GoRoute(
              path: '/sessions',
              pageBuilder: (context, state) => routePage(state, const WorkSessionsPage()),
              routes: [
                GoRoute(parentNavigatorKey: _root, path: 'new', pageBuilder: (context, state) => routePage(state, const NewSessionPage())),
              ],
            ),
          ]),
          StatefulShellBranch(routes: [
            GoRoute(path: '/inbox', pageBuilder: (context, state) => routePage(state, const InboxPage())),
          ]),
          StatefulShellBranch(routes: [
            GoRoute(path: '/board', pageBuilder: (context, state) => routePage(state, const BoardTab())),
          ]),
          StatefulShellBranch(routes: [
            GoRoute(path: '/more', pageBuilder: (context, state) => routePage(state, const MorePage())),
          ]),
        ],
      ),
      // The Team tab is gone (ticket #71): an old place of it opens Sessions.
      GoRoute(path: '/team', redirect: (context, state) => '/sessions'),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/session/:cid',
        pageBuilder: (context, state) => routePage(state, SessionPage(conversationId: state.pathParameters['cid']!)),
      ),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/board/new',
        pageBuilder: (context, state) => routePage(state, NewTicketPage(projectId: state.uri.queryParameters['project'], draft: state.extra is TicketDraft ? state.extra as TicketDraft : null)),
      ),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/ticket/:tid',
        pageBuilder: (context, state) => routePage(state, TicketPage(ticketId: state.pathParameters['tid']!, discussion: state.uri.queryParameters['tab'] == 'discussion', activity: state.uri.queryParameters['tab'] == 'activity', message: state.uri.queryParameters['message'])),
        routes: [
          GoRoute(parentNavigatorKey: _root, path: 'plan', pageBuilder: (context, state) => routePage(state, TicketPlanPage(ticketId: state.pathParameters['tid']!))),
          GoRoute(parentNavigatorKey: _root, path: 'report', pageBuilder: (context, state) => routePage(state, TicketReportPage(ticketId: state.pathParameters['tid']!))),
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
      // Where a link to a message in a channel lands (A5a); a ticket's channel opens the ticket.
      GoRoute(
        parentNavigatorKey: _root,
        path: '/channel/:cid',
        pageBuilder: (context, state) => routePage(state, ChannelLinkPage(conversationId: state.pathParameters['cid']!, message: state.uri.queryParameters['message'], thread: state.uri.queryParameters['thread'])),
      ),
      GoRoute(parentNavigatorKey: _root, path: '/projects', pageBuilder: (context, state) => routePage(state, const ProjectsPage())),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/settings',
        pageBuilder: (context, state) => routePage(state, const SettingsPage()),
        routes: [
          GoRoute(parentNavigatorKey: _root, path: 'board', pageBuilder: (context, state) => routePage(state, const BoardSettingsPage())),
          GoRoute(parentNavigatorKey: _root, path: 'employee-runs', pageBuilder: (context, state) => routePage(state, const EmployeeRunsPage())),
          GoRoute(
            parentNavigatorKey: _root,
            path: 'board-members',
            pageBuilder: (context, state) => routePage(state, const BoardMembersPage()),
            routes: [
              GoRoute(parentNavigatorKey: _root, path: 'new', pageBuilder: (context, state) => routePage(state, const NewBoardMemberPage())),
              GoRoute(
                parentNavigatorKey: _root,
                path: ':uid/sessions',
                pageBuilder: (context, state) => routePage(state, SessionsPage(userId: state.pathParameters['uid'], username: state.uri.queryParameters['name'])),
              ),
              GoRoute(
                parentNavigatorKey: _root,
                path: ':uid/password',
                pageBuilder: (context, state) => routePage(state, TemporaryPasswordPage(userId: state.pathParameters['uid']!, username: state.uri.queryParameters['name'] ?? 'this board member')),
              ),
            ],
          ),
        ],
      ),
      GoRoute(
        parentNavigatorKey: _root,
        path: '/approval/:aid',
        pageBuilder: (context, state) => routePage(state, ApprovalPage(approvalId: state.pathParameters['aid']!)),
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
    } else if (_openPersonal(data, repository)) {
      return;
    } else if (taskId != null && projectId != null) {
      config.go('/sessions');
      config.push('/project/$projectId/task/$taskId');
    } else {
      config.go('/inbox');
    }
  }

  /// A push of the personal Inbox: the ticket or channel it names, at its place
  /// (A3). It carries ids and the server's words only. False when it names neither.
  bool _openPersonal(Map<String, dynamic> data, AureliusRepository repository) {
    final messageId = data['message_id'] as String?;
    final place = pushPlace(data);
    if (place == null) return false;
    config.go(data['ticket_id'] != null ? '/board' : '/sessions');
    // A reply in a thread has its own place; only the server knows it.
    final context = _root.currentContext;
    if (messageId != null && context != null && context.mounted && repository.linkState.has(messageLocateFeature)) {
      unawaited(openMessageAt(context, repository, messageId));
    } else {
      config.push(place);
    }
    return true;
  }

  /// Open a link from outside a page, once this host's app is up (a switch of
  /// server, A5a; the operating system, A5b). False while there is no page yet.
  bool openLink(String raw) {
    final context = _root.currentContext;
    if (context == null || !context.mounted) return false;
    unawaited(openAureliusLink(context, raw));
    return true;
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

  void dispose() {
    config.dispose();
  }
}

/// Flutter's own MaterialPage, so the theme's page transitions apply: the iOS
/// edge swipe back and the slide. go_router 18 looks for the MaterialApp of
/// the material_ui package; ours is Flutter's, so it would fall back to plain
/// pages without a transition or back gesture. The page's name is the route
/// pattern ("/ticket/:tid/plan").
Page<void> routePage(GoRouterState state, Widget child) =>
    MaterialPage<void>(key: state.pageKey, name: state.fullPath ?? state.name ?? state.path, restorationId: state.pageKey.value, child: child);
