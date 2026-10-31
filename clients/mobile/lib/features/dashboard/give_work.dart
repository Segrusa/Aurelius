import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/commands.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../project/new_task_page.dart';

/// The project and lead work was last given to, kept on this phone per host so
/// Give work starts there next time. Loaded once; ids that no longer exist are
/// ignored by the forms.
class LastGiven {
  LastGiven._();

  static String? _host;
  static String? project;
  static String? lead;

  static String _key(String hostId) => 'give.last.$hostId';

  /// Read the saved choice of [hostId] (once per host).
  static Future<void> load(String hostId) async {
    if (_host == hostId) return;
    _host = hostId;
    project = null;
    lead = null;
    try {
      final saved = (await SharedPreferences.getInstance()).getStringList(_key(hostId)) ?? const [];
      if (_host != hostId) return;
      project = saved.elementAtOrNull(0)?.nonEmpty;
      lead = saved.elementAtOrNull(1)?.nonEmpty;
    } catch (_) {
      // No preferences (tests, a broken store): the defaults.
    }
  }

  /// Remember [projectId] and [employeeId] as the last choice on [hostId].
  static Future<void> remember(String hostId, {required String projectId, String? employeeId}) async {
    _host = hostId;
    project = projectId;
    lead = employeeId;
    try {
      await (await SharedPreferences.getInstance()).setStringList(_key(hostId), [projectId, employeeId ?? '']);
    } catch (_) {
      // Not saved; it still holds until the app restarts.
    }
  }

  /// The host of [context]'s repository; null without one (tests).
  static String? hostOf(BuildContext context) {
    try {
      return context.read<AureliusRepository>().host.hostId;
    } catch (_) {
      return null;
    }
  }
}

extension on String {
  String? get nonEmpty => isEmpty ? null : this;
}

/// The Board gives work: the new-task screen with a project field, the last
/// project and lead chosen preselected (else the single root employee).
/// Creates the task and opens it.
/// [title], [employeeId] and [projectId] fill the form (Home's Give work box).
Future<void> giveWork(BuildContext context, {String? title, String? employeeId, String? projectId}) async {
  final host = LastGiven.hostOf(context);
  if (host != null && (employeeId == null || projectId == null)) await LastGiven.load(host);
  if (!context.mounted) return;
  final company = context.read<CompanyBloc>().state;
  final lastLead = company.activeEmployees.any((e) => e.id == LastGiven.lead) ? LastGiven.lead : null;
  final lastProject = company.openProjects.any((p) => p.id == LastGiven.project) ? LastGiven.project : null;
  final task = await NewTaskPage.open(
    context,
    company.activeEmployees,
    projects: company.openProjects,
    projectId: projectId ?? lastProject,
    employeeId: employeeId ?? lastLead,
    title: title,
  );
  if (task == null || task.projectId == null || !context.mounted) return;
  await sendTask(context, task.projectId!, task);
}

/// Create [task] in [projectId] and open it; a notice if it could not be sent.
Future<void> sendTask(BuildContext context, String projectId, NewTask task) async {
  final repository = context.read<AureliusRepository>();
  final router = GoRouter.of(context);
  try {
    final receipt = await repository.write(
      Commands.createTask(projectId, task.title, task.goal, task.criteria, employeeId: task.employeeId, run: task.run, files: task.files, variants: task.variants),
    );
    if (!context.mounted) return;
    if (LastGiven.hostOf(context) case final host?) LastGiven.remember(host, projectId: projectId, employeeId: task.employeeId);
    toast(context, task.variants > 1 ? 'Task created; it is tried ${task.variants} ways first.' : 'Task created.');
    if (receipt.command case CommandResultTaskCreated(:final taskId)) router.push('/project/$projectId/task/$taskId');
  } on NotSentException catch (e) {
    if (context.mounted) toast(context, e.toString());
  } on ProtocolException catch (e) {
    if (context.mounted) toast(context, e.message);
  }
}
