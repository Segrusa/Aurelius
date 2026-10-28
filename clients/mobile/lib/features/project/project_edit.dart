import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/widgets/dialogs.dart';
import '../../data/commands.dart';
import '../../data/views.dart';
import '../connection/connection_bloc.dart';
import 'project_bloc.dart';

/// The server's limit for a project description (`project.set_description`).
const maxDescription = 2000;

/// "Edit…" in the project menu: name, description and work folder in one
/// form. Only what changed is sent; the first command carries the project's
/// version. Older servers have no command for the description, so it is left
/// out of the form there.
Future<void> editProject(BuildContext context, ProjectSummary project) async {
  final bloc = context.read<ProjectBloc>();
  final editableDescription = context.read<ConnectionBloc>().state.features.contains('project_description');
  final description = project.description ?? '';
  final folder = project.folder ?? '';
  final values = await promptForm(
    context,
    title: 'Edit project',
    fields: [
      FieldSpec('Name', initial: project.name),
      if (editableDescription) FieldSpec('Description', initial: description, required: false, lines: 4, maxLength: maxDescription),
      FieldSpec('Folder on the PC', initial: folder, hint: 'Where its tasks run', required: false),
    ],
  );
  if (values == null) return;
  final name = values[0].trim();
  final text = editableDescription ? values[1].trim() : description;
  final path = values.last.trim();
  final commands = <Map<String, dynamic>>[
    if (name.isNotEmpty && name != project.name) Commands.renameProject(project.id, name),
    if (editableDescription && text != description.trim()) Commands.setProjectDescription(project.id, text),
    if (path.isNotEmpty && path != folder) Commands.setProjectFolder(project.id, path),
  ];
  if (commands.isEmpty) return;
  bloc.send(commands.first, expectedVersion: project.version, then: commands.skip(1).toList(), done: 'Saved.');
}
