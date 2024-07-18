import * as vscode from 'vscode';

export function convertLocation(obj: any): vscode.Location {
    const uri = vscode.Uri.parse(obj.uri.toString());
    const range = new vscode.Range(
        new vscode.Position(obj.range.start.line, obj.range.start.character),
        new vscode.Position(obj.range.end.line, obj.range.end.character)
    );

    return new vscode.Location(uri, range);
}