import '../core/net/protocol.dart';
import 'aurelius_repository.dart';
import 'vault.dart';
import 'views.dart';

/// The writes whose params take `confirm_secret` (B10, AUR-MU-04-F5): sent
/// again with it, the PC stores the text with each value redacted. Names,
/// instructions, memories and paths stay refused, even with the flag.
const confirmSecretMethods = {
  Methods.employeeSendMessage,
  Methods.chatSend,
  Methods.workSessionStart,
  Methods.workSessionDispatch,
  Methods.ticketPost,
  Methods.messageEdit,
  Methods.projectConversationSend,
  Methods.taskCreate,
  Methods.ticketCreate,
  Methods.ticketUpdate,
  Methods.ticketRevisePlan,
  Methods.ticketSendBack,
  Methods.ticketAccept,
  Methods.approvalDecidePlan,
  Methods.approvalAnswer,
};

/// [request] may be sent again with `confirm_secret`.
bool takesConfirmSecret(Map<String, dynamic> request) => confirmSecretMethods.contains(request['method']);

/// [request] with `confirm_secret: true` in its params.
Map<String, dynamic> withConfirmSecret(Map<String, dynamic> request) => {
      ...request,
      'params': {...(request['params'] as Map<String, dynamic>? ?? const {}), 'confirm_secret': true},
    };

/// Asks the board member about the vault items [items] (names only).
/// [canSendAnyway] false: the command has no `confirm_secret`, so it only says
/// what to take out. True: send anyway.
typedef SendAnywayAsk = Future<bool> Function(List<String> items, {required bool canSendAnyway});

/// Writes [request]. Refused with `looks_like_secret`, it asks once with
/// [ask]; "Send anyway" writes it again with `confirm_secret` (a new request,
/// so a new idempotency key). Null: not sent, the board member chose so.
/// Refused again after Send anyway (a name, a path, a value in a file) it
/// throws a [ProtocolException] with [secretCheckNoSendAnyway] and never asks
/// again: no loop.
Future<Receipt?> writeConfirmingSecret(
  AureliusRepository repository,
  Map<String, dynamic> request,
  SendAnywayAsk ask, {
  int? expectedVersion,
  bool retry = true,
}) async {
  try {
    return await repository.write(request, expectedVersion: expectedVersion, retry: retry);
  } on ProtocolException catch (e) {
    if (!e.looksLikeSecret) rethrow;
    final confirmable = takesConfirmSecret(request);
    final send = await ask(e.matchedItems, canSendAnyway: confirmable);
    if (!send || !confirmable) return null;
  }
  try {
    return await repository.write(withConfirmSecret(request), expectedVersion: expectedVersion, retry: retry);
  } on ProtocolException catch (e) {
    if (!e.looksLikeSecret) rethrow;
    throw ProtocolException(e.code, secretCheckNoSendAnyway, matchedItems: e.matchedItems);
  }
}
