import * as vscode from 'vscode';
import { EffectResponseData } from './file_tree_view';

export interface AuditResponse {
    effects: [EffectResponseData, string][];
}

export interface AuditNotification {
    safety_annotation: string;
    effect: EffectResponseData;
}

export class AuditAnnotations implements vscode.CodeLensProvider {    
    private effects: { [file: string]: EffectResponseData[] } = {};
    private onDidChangeCodeLensesEmitter = new vscode.EventEmitter<void>();
    readonly onDidChangeCodeLenses = this.onDidChangeCodeLensesEmitter.event;

    prevAnnotations: Map<EffectResponseData, string> = new Map();

    provideCodeLenses(document: vscode.TextDocument, __token: vscode.CancellationToken): vscode.CodeLens[] {
        let annotations: vscode.CodeLens[] = [];
        const fileEffects = this.effects[document.uri.fsPath]; 

        if (Object.keys(this.effects).length === 0 || !fileEffects) {
            return annotations;
        }
        
        for (const effect of fileEffects) {
            const range = effect.location.range;
            const prevAnnotation = this.prevAnnotations.get(effect) || '';

            const safe: vscode.Command = {
                title: prevAnnotation === 'Safe' ? 'Marked as [[  SAFE  ]]' : '✔️ Safe',
                command: 'cargo-scan.set_annotation',
                arguments: [effect, 'Safe'],
                tooltip: `Effect Instance : ${effect.callee}`
            };
    
            const unsafe: vscode.Command = {
                title: prevAnnotation === 'Unsafe' ? 'Marked as [[  UNSAFE  ]]' : '❗ Unsafe',
                command: 'cargo-scan.set_annotation',
                arguments: [effect, 'Unsafe'],
                tooltip: `Effect Instance : ${effect.callee}`
            };
    
            const cc: vscode.Command = {
                title: prevAnnotation === 'Caller-checked' ? 'Marked as [[ CALLER-CHECKED ]]' : '❔ Caller-Checked',
                command: 'cargo-scan.get_callers',
                arguments: [effect],
                tooltip: `Effect Instance : ${effect.callee}`
            };
            annotations.push(new vscode.CodeLens(range, safe));
            annotations.push(new vscode.CodeLens(range, unsafe));
            annotations.push(new vscode.CodeLens(range, cc));
        }

        return annotations;
    }

    resolveCodeLens?(codeLens: vscode.CodeLens, __token: vscode.CancellationToken) {
        return codeLens;
    }

    refresh() {
        this.onDidChangeCodeLensesEmitter.fire();
    }

    clear() {
        this.effects = {};
        this.refresh();
    }

    setPreviousAnnotations(
        effects: { [file: string]: EffectResponseData[] },
        prevAnnotations: Map<EffectResponseData, string>
    ) {
        this.effects = effects;
        this.prevAnnotations = prevAnnotations;
        this.refresh();
    }

    trackUserAnnotations(effect: EffectResponseData, ann: string) {
        this.prevAnnotations.set(effect, ann);
        this.refresh();
    }

    register(context: vscode.ExtensionContext) {
        context.subscriptions.push(
            vscode.languages.registerCodeLensProvider({ pattern: '**/*' }, this)
        );
    }

    showCallers(effect: EffectResponseData, callers: EffectResponseData[]) {
        const uri = effect.location.uri;
        const line = effect.location.range.start.line;
        const col = effect.location.range.start.character;
        const position = new vscode.Position(line, col);
        let locations: vscode.Location[] = callers.map(caller => caller.location);

        vscode.commands.executeCommand('editor.action.peekLocations', uri, position, locations, 'peek');
    }
}