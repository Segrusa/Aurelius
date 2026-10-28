import 'package:flutter/material.dart';

import '../theme/tokens.dart';

class FieldSpec {
  const FieldSpec(this.label, {this.initial = '', this.hint, this.required = true, this.lines = 1, this.maxLength, this.options, this.dropdown = false});

  final String label;
  final String initial;
  final String? hint;
  final bool required;
  final int lines;

  /// Caps the text field and shows a counter.
  final int? maxLength;

  /// When set, a segmented choice (value -> label) instead of a text field; [initial] is a value.
  final Map<String, String>? options;

  /// With [options]: a dropdown (many choices) instead of the segmented buttons.
  final bool dropdown;
}

/// A small form in a dialog; returns the values or `null` when cancelled.
Future<List<String>?> promptForm(BuildContext context, {required String title, required List<FieldSpec> fields, String action = 'Save'}) {
  final controllers = [for (final f in fields) TextEditingController(text: f.initial)];
  return showDialog<List<String>>(
    context: context,
    builder: (dialog) => StatefulBuilder(
      builder: (dialog, setState) {
        final valid = [for (final (i, f) in fields.indexed) !f.required || controllers[i].text.trim().isNotEmpty].every((v) => v);
        return AlertDialog(
          title: Text(title),
          // Fill the dialog's width so text fields are not squeezed.
          content: SizedBox(
            width: double.maxFinite,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  for (final (i, f) in fields.indexed) ...[
                    if (i > 0) const SizedBox(height: 12),
                    if (f.options != null && f.dropdown)
                      DropdownButtonFormField<String>(
                        initialValue: f.options!.containsKey(controllers[i].text) ? controllers[i].text : f.options!.keys.first,
                        isExpanded: true,
                        decoration: InputDecoration(labelText: f.label),
                        items: [for (final o in f.options!.entries) DropdownMenuItem(value: o.key, child: Text(o.value, overflow: TextOverflow.ellipsis))],
                        onChanged: (v) => setState(() => controllers[i].text = v ?? ''),
                      )
                    else if (f.options != null)
                      SegmentedButton<String>(
                        showSelectedIcon: false,
                        segments: [for (final o in f.options!.entries) ButtonSegment(value: o.key, label: Text(o.value))],
                        selected: {controllers[i].text},
                        onSelectionChanged: (s) => setState(() => controllers[i].text = s.first),
                      )
                    else
                      TextField(
                      controller: controllers[i],
                      autofocus: i == 0,
                      minLines: f.lines,
                      maxLines: f.lines == 1 ? 1 : f.lines + 4,
                      maxLength: f.maxLength,
                      textCapitalization: TextCapitalization.sentences,
                      decoration: InputDecoration(labelText: f.required ? f.label : '${f.label} (optional)', hintText: f.hint),
                      onChanged: (_) => setState(() {}),
                    ),
                  ],
                ],
              ),
            ),
          ),
          actions: [
            TextButton(onPressed: () => Navigator.pop(dialog), child: const Text('Cancel')),
            FilledButton(onPressed: valid ? () => Navigator.pop(dialog, [for (final c in controllers) c.text.trim()]) : null, child: Text(action)),
          ],
        );
      },
    ),
  );
}

Future<String?> promptText(BuildContext context, {required String title, required String label, String initial = '', int lines = 1, String action = 'Save'}) async {
  final values = await promptForm(context, title: title, fields: [FieldSpec(label, initial: initial, lines: lines)], action: action);
  return values?.first;
}

Future<bool> confirm(BuildContext context, {required String title, String? body, String action = 'OK', bool danger = false}) async =>
    await showDialog<bool>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: Text(title),
        content: body == null ? null : Text(body),
        actions: [
          TextButton(onPressed: () => Navigator.pop(dialog, false), child: const Text('Cancel')),
          TextButton(
            onPressed: () => Navigator.pop(dialog, true),
            style: danger ? TextButton.styleFrom(foregroundColor: Tokens.negative) : null,
            child: Text(action),
          ),
        ],
      ),
    ) ??
    false;
