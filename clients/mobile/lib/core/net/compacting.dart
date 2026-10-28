import 'package:flutter/foundation.dart';

/// The conversations whose run compacts its context right now (stream updates with
/// `progress.compacting`), so a task's mark can say so wherever it shows. Servers
/// without feature `compact_status` never set it: the set stays empty.
/// What a run shows while it compacts the conversation's context.
const compactingLabel = 'Context comprimeren…';

final compactingConversations = ValueNotifier<Set<String>>(const {});

void noteCompacting(String conversationId, bool? on) {
  final now = compactingConversations.value;
  if ((on ?? false) == now.contains(conversationId)) return;
  compactingConversations.value = on == true ? {...now, conversationId} : ({...now}..remove(conversationId));
}
