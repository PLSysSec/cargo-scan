import * as vscode from 'vscode';
import { EffectResponseData } from './file_tree_view';
import { rangeToString } from './util';

export interface AuditResponse {
    effects: Map<EffectResponseData, [EffectResponseData, string][]>;
}

export interface AuditNotification {
    safety_annotation: string;
    effect: EffectResponseData;
}

export class AuditAnnotations implements vscode.CodeLensProvider {    
    private selected?: EffectResponseData;
    private annotations: vscode.CodeLens[] = [];
    private effects: { [file: string]: EffectResponseData[] } = {};
    private prevAnnotations: Map<EffectResponseData, string> = new Map();
    private onDidChangeCodeLensesEmitter = new vscode.EventEmitter<void>();
    readonly onDidChangeCodeLenses = this.onDidChangeCodeLensesEmitter.event;

    provideCodeLenses(document: vscode.TextDocument, __token: vscode.CancellationToken): vscode.CodeLens[] {
        this.annotations = [];
        const fileEffects = this.effects[document.uri.fsPath]; 

        if (Object.keys(this.effects).length === 0 || !fileEffects) {
            return this.annotations;
        }
        
        const groupedEffects = this.groupByRange(fileEffects);
        for (const rangeStr in groupedEffects) {
            const effects = groupedEffects[rangeStr];
            const range = effects[0].location.range;

            if (effects.length > 1) {
                const multiples: vscode.Command = {
                    title: 'Preview propagated effects',
                    command: 'cargo-scan.view_annotations',
                    arguments: [effects],
                    tooltip: `Multiple base effects are propagated to this location. Choose which one you want to audit.`
                };
                this.annotations.push(new vscode.CodeLens(range, multiples));
                this.annotations.push(new vscode.CodeLens(range));
                this.annotations.push(new vscode.CodeLens(range));
                this.annotations.push(new vscode.CodeLens(range));
                continue;
            }
            
            const effect = effects[0];
            const prevAnnotation = this.prevAnnotations.get(effect) || '';

            const safe: vscode.Command = {
                title: prevAnnotation === 'Safe' ? 'Marked as SAFE' : '✔️ Safe',
                command: 'cargo-scan.set_annotation',
                arguments: [effect, 'Safe'],
                tooltip: `Effect Instance : ${effect.callee}`
            };
    
            const unsafe: vscode.Command = {
                title: prevAnnotation === 'Unsafe' ? 'Marked as UNSAFE' : '❗ Unsafe',
                command: 'cargo-scan.set_annotation',
                arguments: [effect, 'Unsafe'],
                tooltip: `Effect Instance : ${effect.callee}`
            };
    
            const cc: vscode.Command = {
                title: prevAnnotation === 'Caller-checked' ? 'Marked as CALLER-CHECKED' : '❔ Caller-Checked',
                command: 'cargo-scan.get_callers',
                arguments: [effect],
                tooltip: `Effect Instance : ${effect.callee}`
            };
            this.annotations.push(new vscode.CodeLens(range, safe));
            this.annotations.push(new vscode.CodeLens(range, unsafe));
            this.annotations.push(new vscode.CodeLens(range, cc));
        }

        return this.annotations;
    }

    resolveCodeLens?(codeLens: vscode.CodeLens, __token: vscode.CancellationToken) {
        if (!this.selected) {
            return codeLens;
        }

        const effect = this.selected;
        if (rangeToString(codeLens.range) !== rangeToString(effect.location.range)) {
            return codeLens;
        }
        
        let cmdIndex = this.annotations.indexOf(codeLens) % 3;
        const prevAnnotation = this.prevAnnotations.get(effect) || '';
        
        switch (cmdIndex) {
            case 1:
                codeLens.command = {
                    title: prevAnnotation === 'Safe' ? 'Marked as SAFE' : '✔️ Safe',
                    command: 'cargo-scan.set_annotation',
                    arguments: [effect, 'Safe'],
                    tooltip: `Effect Instance : ${effect.callee}`
                };
                break;
            case 2:
                codeLens.command = {
                    title: prevAnnotation === 'Unsafe' ? 'Marked as UNSAFE' : '❗ Unsafe',
                    command: 'cargo-scan.set_annotation',
                    arguments: [effect, 'Unsafe'],
                    tooltip: `Effect Instance : ${effect.callee}`
                };
                break;
            case 0:
                codeLens.command = {
                    title: prevAnnotation === 'Caller-checked' ? 'Marked as CALLER-CHECKED' : '❔ Caller-Checked',
                    command: 'cargo-scan.get_callers',
                    arguments: [effect],
                    tooltip: `Effect Instance : ${effect.callee}`
                };
                break;
        }
        return codeLens;
    }

    refresh() {
        this.onDidChangeCodeLensesEmitter.fire();
    }

    clear() {
        this.selected = undefined;
        this.annotations = [];
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
        this.selected = undefined;
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

    private groupByRange(effects: EffectResponseData[]): { [range: string]: EffectResponseData[] } {
        let groupedEffects:  { [range: string]: EffectResponseData[] } = {};
        for (const effect of effects) {
            const range = rangeToString(effect.location.range);
            if (!groupedEffects[range]) {
                groupedEffects[range] = [];
            }
            groupedEffects[range].push(effect);
        }

        return groupedEffects;
    }

    effectToAudit(selected: EffectResponseData | undefined) {
        this.selected = selected;
    }
}