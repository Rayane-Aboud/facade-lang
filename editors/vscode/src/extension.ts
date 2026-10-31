import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import { parseUnits, UnitDecl, wantsPlusMinus, wantsUnit } from "./units";

export function activate(context: vscode.ExtensionContext): void {
  context.subscriptions.push(
    vscode.languages.registerCompletionItemProvider(
      "facadelang",
      { provideCompletionItems },
      " ", "[", "*", "/", "-",
    ),
  );
}

function provideCompletionItems(
  document: vscode.TextDocument,
  position: vscode.Position,
): vscode.CompletionItem[] {
  const before = document.lineAt(position.line).text.slice(0, position.character);
  const config = vscode.workspace.getConfiguration("facadelang");

  if (wantsPlusMinus(before) && config.get<boolean>("suggestPlusMinusSymbol", false)) {
    return [plusMinusItem(position)];
  }
  if (!wantsUnit(before)) {
    return [];
  }
  const units = [
    ...readPrelude(document, config),
    ...parseUnits(document.getText(), path.basename(document.fileName)),
  ];
  return units.map(unitItem);
}


function readPrelude(
  document: vscode.TextDocument,
  config: vscode.WorkspaceConfiguration,
): UnitDecl[] {
  const setting = config.get<string>("preludePath", "prelude/std.fcd");
  const folder = vscode.workspace.getWorkspaceFolder(document.uri);
  if (!path.isAbsolute(setting) && folder === undefined) {
    return [];
  }
  const file = path.isAbsolute(setting) ? setting : path.join(folder!.uri.fsPath, setting);
  if (file === document.fileName) {
    return []; // editing the prelude itself: its units come from the document
  }
  try {
    return parseUnits(fs.readFileSync(file, "utf8"), path.basename(file));
  } catch {
    return [];
  }
}

function unitItem(unit: UnitDecl): vscode.CompletionItem {
  const item = new vscode.CompletionItem(unit.name, vscode.CompletionItemKind.Unit);
  item.detail = unit.meaning;
  item.documentation = `Declared in ${unit.source}`;
  return item;
}

function plusMinusItem(position: vscode.Position): vscode.CompletionItem {
  const item = new vscode.CompletionItem("±", vscode.CompletionItemKind.Operator);
  item.detail = "tolerance (same as +/-)";
  item.filterText = "+/-";
  item.range = new vscode.Range(position.translate(0, -3), position);
  return item;
}
