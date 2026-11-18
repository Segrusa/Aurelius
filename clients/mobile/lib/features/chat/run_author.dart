import '../company/company_bloc.dart';

/// The name of the employee whose agent runs [runId], if one does now: who
/// writes a streaming answer in a chat where several answer.
String? runAuthorOf(CompanyState company, String runId) =>
    company.employees.where((m) => m.agents.any((a) => a.runId == runId)).firstOrNull?.name;
