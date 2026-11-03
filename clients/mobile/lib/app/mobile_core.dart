import 'package:flutter/foundation.dart' show visibleForTesting;

/// What the phone leaves out of its menus so it stays the core: Team chat, Inbox, Board,
/// the chat with employees and Settings (ticket #19). Only the entry points go: the routes,
/// the screens and their code stay, so a deep link or a push to one of them still opens it.
/// Bringing a screen back is deleting its id here.
const kHiddenInMobileCore = <String>{
  'projects',
  'tasks',
  'ideas',
  'vault',
  'guidelines',
  'company',
  'plugins',
  'usage',
  'updates',
  'advanced',
  'recurring',
  'artifacts',
  'context',
  'load',
  'power',
  'runtime',
};

/// What the menus leave out right now: [kHiddenInMobileCore], except that a test of a hidden screen empties it.
@visibleForTesting
Set<String> mobileCoreHidden = kHiddenInMobileCore;

/// True when the menus show the screen [id] (an id in [kHiddenInMobileCore] is not shown).
bool shownInMobileCore(String id) => !mobileCoreHidden.contains(id);
