import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/discussion.dart';
import '../../data/quick_task.dart';
import '../../data/secret_write.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';
import '../board/ticket_actions.dart';
import '../chat/secret_check.dart';
import '../company/company_bloc.dart';

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

/// Gives work from anywhere: the New ticket form, the last project and lead
/// chosen preselected. A task only exists under a ticket.
/// [title], [employeeId] and [projectId] fill the form (Home's Give work box);
/// [quick] opens it as a quick task (ticket #53), whose text is the description.
Future<void> giveWork(BuildContext context, {String? title, String? employeeId, String? projectId, bool quick = false}) async {
  final host = LastGiven.hostOf(context);
  if (host != null && (employeeId == null || projectId == null)) await LastGiven.load(host);
  if (!context.mounted) return;
  final company = context.read<CompanyBloc>().state;
  final lastLead = company.activeEmployees.any((e) => e.id == LastGiven.lead) ? LastGiven.lead : null;
  final lastProject = company.openProjects.any((p) => p.id == LastGiven.project) ? LastGiven.project : null;
  GoRouter.of(context).push('/board/new',
      extra: quick
          ? TicketDraft(title: '', description: title ?? '', projectId: projectId ?? lastProject, ownerId: employeeId, quick: true)
          : TicketDraft(title: title ?? '', description: '', projectId: projectId ?? lastProject, ownerId: employeeId ?? lastLead));
}

/// The ticket of Home's Give work box: for [employeeId] (none: the dispatcher
/// chooses), started at once. With [quick] a quick task (ticket #53): the text
/// is its description, the server makes the title, and [employeeId] is required.
Map<String, dynamic> givenWorkTicket(String projectId, String title, String? employeeId, {String? responsibleUserId, bool quick = false}) => quick && employeeId != null
    ? quickTaskRequest(projectId: projectId, description: title, employeeId: employeeId, responsibleUserId: responsibleUserId)
    : ticketCreate(projectId: projectId, title: title, ownerEmployeeId: employeeId, responsibleUserId: responsibleUserId, start: true);

/// Make and open the ticket of Home's Give work box; false when it was not made
/// (a notice, or the B10 dialog), so the caller can give the typed text back.
Future<bool> sendGivenWork(BuildContext context, String projectId, String title, String? employeeId, {bool quick = false}) async {
  final repository = context.read<AureliusRepository>();
  final router = GoRouter.of(context);
  final responsible = repository.linkState.has(ticketResponsibleFeature) ? repository.linkState.user?.id : null;
  try {
    final receipt = await writeConfirmingSecret(repository, givenWorkTicket(projectId, title, employeeId, responsibleUserId: responsible, quick: quick), sendAnywayDialog(context));
    if (receipt == null || !context.mounted) return false;
    if (LastGiven.hostOf(context) case final host?) {
      if (quick && employeeId != null) {
        LastQuickEmployee.remember(host, employeeId);
      } else {
        LastGiven.remember(host, projectId: projectId, employeeId: employeeId);
      }
    }
    if (receipt.command case CommandResultTicketCreated(:final ticketId, :final number)) {
      toast(context, quick ? 'Quick task #$number started.' : '#$number created and started.');
      router.push(ticketChatPlace(ticketId));
    }
    return true;
  } on Object catch (e) {
    if (context.mounted) toast(context, ticketFailure(e));
    return false;
  }
}
