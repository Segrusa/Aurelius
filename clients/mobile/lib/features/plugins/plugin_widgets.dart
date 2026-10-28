import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../data/views.dart';

/// "2 skills", "hooks", ...: one small chip per component kind.
class PluginChips extends StatelessWidget {
  const PluginChips(this.components, {super.key});

  /// Null: the marketplace does not show them before the plugin is installed.
  final PluginComponents? components;

  @override
  Widget build(BuildContext context) {
    final c = components;
    if (c == null) return Text('Components unknown until installed.', style: TextStyle(color: Tokens.muted, fontSize: 12.5));
    if (c.chips.isEmpty) return Text('No components found.', style: TextStyle(color: Tokens.muted, fontSize: 12.5));
    return Wrap(spacing: 6, runSpacing: 6, children: [for (final label in c.chips) _Chip(label)]);
  }
}

class _Chip extends StatelessWidget {
  const _Chip(this.label);

  final String label;

  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(color: Tokens.fill, borderRadius: BorderRadius.circular(8)),
        child: Text(label, style: TextStyle(color: Tokens.text2, fontSize: 12)),
      );
}

/// The hooks warning and the MCP note for a plugin's components.
class PluginNotes extends StatelessWidget {
  const PluginNotes(this.components, {super.key});

  final PluginComponents? components;

  @override
  Widget build(BuildContext context) {
    final c = components;
    if (c == null) return const SizedBox.shrink();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (c.hooks)
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Icon(Icons.warning_amber_rounded, size: 16, color: Tokens.warning),
              const SizedBox(width: 6),
              Expanded(child: Text(pluginHooksWarning, style: TextStyle(color: Tokens.warning, fontSize: 13, fontWeight: FontWeight.w600))),
            ]),
          ),
        if (c.mcpServers.isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Text('$pluginMcpNote (${c.mcpServers.join(', ')})', style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
          ),
      ],
    );
  }
}
