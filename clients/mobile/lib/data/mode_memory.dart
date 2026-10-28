import 'package:shared_preferences/shared_preferences.dart';

/// The mode an employee was last given on this phone ("Ask", "Auto"…, the
/// wire value), per PC. It is the default for the next task or conversation
/// with that employee. Kept on the phone only; nothing is sent. Every call
/// falls back quietly (no stored value, no preferences in tests).
abstract final class ModeMemory {
  static String _key(String hostId, String employeeId) => 'mode.$hostId.$employeeId';

  static Future<String?> read(String hostId, String employeeId) async {
    try {
      return (await SharedPreferences.getInstance()).getString(_key(hostId, employeeId));
    } on Object {
      return null;
    }
  }

  /// Remembers [mode]; `null` forgets it (back to the employee's default).
  static Future<void> write(String hostId, String employeeId, String? mode) async {
    try {
      final prefs = await SharedPreferences.getInstance();
      if (mode == null) {
        await prefs.remove(_key(hostId, employeeId));
      } else {
        await prefs.setString(_key(hostId, employeeId), mode);
      }
    } on Object {
      // Not remembered; the next task simply starts from the default.
    }
  }
}
