import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../data/secret_write.dart';
import '../../data/vault.dart';
import 'chat_bloc.dart';

/// B10 (P-46): the PC refused [unsent] because it holds the value of vault
/// items. Shows only their names; "Send anyway" sends it again with
/// `confirm_secret`, and the PC stores it with the value redacted. "Don't
/// send" leaves it unsent (Discard or Send anyway… later).
Future<void> sendAnywayAsked(BuildContext context, ChatBloc bloc, UnsentMessage unsent) async {
  final send = await askSendAnyway(context, unsent.secretItems);
  if (send && !bloc.isClosed) bloc.add(ChatSecretConfirmed(unsent.localId));
}

/// The B10 dialog: the names of [items] (never a value), [cancel] and "Send
/// anyway". Without [takeOut] it asks; with it, "Send anyway" would only be
/// refused again (no `confirm_secret` for the command), so it says what to
/// take out and offers only [cancel]. True: send anyway.
Future<bool> askSendAnyway(BuildContext context, List<String> items, {String cancel = "Don't send", String? takeOut}) async {
  final send = await showDialog<bool>(
    context: context,
    builder: (dialog) => AlertDialog(
      key: const ValueKey('secret-check'),
      title: Text(takeOut == null ? secretCheckTitle : secretHeldTitle),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(takeOut == null ? secretCheckBody(items) : secretHeldBody(items, takeOut)),
          const SizedBox(height: 10),
          for (final name in items)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Row(children: [
                Icon(Icons.lock, size: 16, color: Tokens.label2),
                const SizedBox(width: 6),
                Expanded(child: Text(name, style: const TextStyle(fontWeight: FontWeight.w600))),
              ]),
            ),
        ],
      ),
      actions: [
        TextButton(key: const ValueKey('secret-check-cancel'), onPressed: () => Navigator.pop(dialog, false), child: Text(cancel)),
        if (takeOut == null) TextButton(key: const ValueKey('secret-check-send'), onPressed: () => Navigator.pop(dialog, true), child: const Text('Send anyway')),
      ],
    ),
  );
  return send == true;
}

/// [askSendAnyway] over [context] as the [SendAnywayAsk] of
/// [writeConfirmingSecret]: Send anyway where the command takes
/// `confirm_secret`, else only [takeOut] and [cancel]. Gone: not sent.
SendAnywayAsk sendAnywayDialog(BuildContext context, {String cancel = 'Cancel', String takeOut = secretCheckNoSendAnyway}) =>
    (items, {required canSendAnyway}) async => context.mounted && await askSendAnyway(context, items, cancel: cancel, takeOut: canSendAnyway ? null : takeOut);
