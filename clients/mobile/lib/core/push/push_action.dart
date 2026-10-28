import '../../data/aurelius_repository.dart';
import '../../data/commands.dart';
import '../../data/views.dart';
import '../net/connection.dart';
import '../net/protocol.dart';

/// What the "Goedkeuren" action on a push notification did. [done] false means
/// nothing was decided and the caller opens the approval instead.
typedef PushApproveResult = ({bool done, String message});

/// Decide the approval a notification points at, from the notification action
/// "Goedkeuren": a plan is approved with edits (the worker still asks for the
/// rest), any other approval is approved once. Questions need an answer, so
/// they are never approved here.
Future<PushApproveResult> approveFromPush(AureliusRepository repository, String approvalId) async {
  const fallback = 'Niet goedgekeurd; bekijk het verzoek.';
  try {
    final approvals = await repository.approvals();
    final approval = approvals.where((a) => a.id == approvalId).firstOrNull;
    if (approval == null || !approval.isPending) return (done: false, message: 'Dit verzoek is al afgehandeld of verlopen.');
    if (approval.isQuestion) return (done: false, message: 'Deze vraag heeft een antwoord nodig.');
    final command = approval.isPlan
        ? Commands.decidePlan(approvalId, PlanDecision.approveEdits)
        : Commands.decideApproval(approvalId, approval.actionDigest, true);
    final receipt = await repository.write(command, retry: false);
    final status = switch (receipt.command) { CommandResultApprovalDecided(:final status) => status, _ => null };
    if (status == ApprovalStatus.expired) return (done: false, message: 'Dit verzoek is verlopen; er is niets goedgekeurd.');
    return (
      done: true,
      message: approval.isPlan ? 'Plan goedgekeurd. Bewerkingen gaan door; de rest vraagt eerst.' : 'Goedgekeurd. Alleen deze actie wordt uitgevoerd.',
    );
  } on NotSentException {
    return (done: false, message: 'Niet verstuurd: de pc was niet bereikbaar. $fallback');
  } on ProtocolException catch (e) {
    final gone = e.code == ErrorCode.validation || e.code == ErrorCode.notFound;
    return (done: false, message: gone ? 'Dit verzoek is gewijzigd of al afgehandeld.' : '${e.message} $fallback');
  } on Object {
    return (done: false, message: 'Goedkeuren is mislukt. $fallback');
  }
}
