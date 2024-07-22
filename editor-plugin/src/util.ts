import * as vscode from 'vscode';
import { execSync } from 'child_process';
import * as path from 'path';

export function convertLocation(obj: any): vscode.Location {
    const uri = vscode.Uri.parse(obj.uri.toString());
    const range = new vscode.Range(
        new vscode.Position(obj.range.start.line, obj.range.start.character),
        new vscode.Position(obj.range.end.line, obj.range.end.character)
    );

    return new vscode.Location(uri, range);
}

export function setEnvironment() {
    try {
        const cmd = process.platform === 'win32' ? 'where' : 'which';
        const rustcPath = execSync(`${cmd} rustc`, {encoding: 'utf8'}).trim();
        const currPath = process.env.PATH || '';
        process.env.PATH = `${path.dirname(rustcPath)}${path.delimiter}${currPath}`;
    } catch (error) {
        throw error;
    }
}