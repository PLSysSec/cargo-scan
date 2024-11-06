import * as vscode from 'vscode';
import { EffectResponseData, EffectsResponse, LocationItem } from './file_tree_view';
import { annotations, client, locationsProvider } from './extension';
import { AuditResponse } from './audit_annotations';
import { convertLocation } from './util';
import { highlightEffectLocations } from './decorations';

export function registerCommands(context: vscode.ExtensionContext) {
    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.scan', async () => {
            const response = await client.sendRequest<EffectsResponse>('cargo-scan.scan');
            context.globalState.update('annotateEffects', false);
            context.globalState.update('chainAudit', false);
            
            const effects = response.effects.map(effect => ({
                ...effect,
                location: convertLocation(effect.location)
            }));

            locationsProvider.clear();
            locationsProvider.setLocations(effects);
            annotations.clear();
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.audit', async () => {
            vscode.window.withProgress(
                {
                    location: vscode.ProgressLocation.Notification,
                    title: "Cargo Scan Audit"
                },
                async (progress) => {
                    progress.report({ message: "Scanning crate..." });
                    const response = await client.sendRequest<AuditResponse>('cargo-scan.audit');
                    
                    context.globalState.update('annotateEffects', true);
                    context.globalState.update('chainAudit', false);
    
                    let effectsMap = new Map<EffectResponseData, string>();
                    let callStackMap = new Map<string, EffectResponseData[]>();
    
                    for (let [baseEffect, callers] of response.effects) {
                        baseEffect.location = convertLocation(baseEffect.location);
                        callers.forEach((e: [EffectResponseData, string]) => {
                            e[0].location = convertLocation(e[0].location);
                            effectsMap.set(e[0], e[1]);
                        });
    
                        const callStack = callers.map((e: [EffectResponseData, string]) => e[0]);
                        callStackMap.set(JSON.stringify(baseEffect), callStack);
                    }
    
                    const auditedEffects = Array.from(effectsMap)
                        .filter(([_, value]) => value !== 'Skipped')
                        .map(([key, _]) => key);
    
                    locationsProvider.clear();
                    locationsProvider.addAuditedEffects(auditedEffects);
                    locationsProvider.setLocations([...effectsMap.keys()], callStackMap);
                    annotations.setPreviousAnnotations(locationsProvider.getGroupedEffects(), effectsMap);
    
                    const editor = vscode.window.activeTextEditor;
                    if (editor) {
                        highlightEffectLocations(editor, locationsProvider.getGroupedEffects());
                    }
                    
                    vscode.window.showInformationMessage("Scan completed -- You can now start auditing!");
                }
            );
        })
    );
    
    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.get_callers', async (effect: EffectResponseData) => {
            const response = await client.sendRequest<EffectsResponse>('cargo-scan.get_callers', 
                { ...effect, location: { uri: effect.location.uri.toString(), range: effect.location.range }});
            
            const callers = response.effects.map(effect => ({
                ...effect,
                location: convertLocation(effect.location)
            }));

            locationsProvider.setLocations(callers);
            locationsProvider.addAuditedEffects([effect]);
            locationsProvider.updateEffectCallStack(effect, callers);
            annotations.trackUserAnnotations(effect, 'Caller-checked');
            annotations.showCallers(effect, callers);
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.create_chain', async () => {
            client.sendRequest('cargo-scan.create_chain');
            
            client.onNotification('cargo-scan.info', (message: string) => {                
                vscode.window.showInformationMessage(
                message, 
                'View Logs',
                // 'Settings'
                ).then(selection => {
                    if (selection === 'View Logs') {
                        client.outputChannel.show();
                    }
                    // else if (selection === 'Settings') {
                    //     vscode.commands.executeCommand('workbench.action.openSettings', '@ext:PLsysSec.cargo-scan');
                    // }
                });
            });

            context.globalState.update('annotateEffects', false);
            context.globalState.update('chainAudit', false);
            locationsProvider.clear();
            annotations.clear();      
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.audit_chain', async () => {
            const response = await client.sendRequest<AuditResponse>('cargo-scan.audit_chain');
            context.globalState.update('annotateEffects', true);
            context.globalState.update('chainAudit', true);
            let effectsMap = new Map<EffectResponseData, string>();    
            let callStackMap = new Map<string, EffectResponseData[]>();  
            
            for (let [baseEffect, callers] of response.effects) {
                baseEffect.location = convertLocation(baseEffect.location);
                callers.forEach((e: [EffectResponseData, string]) => {
                    e[0].location = convertLocation(e[0].location);
                    effectsMap.set(e[0], e[1]);
                });

                const callStack = callers.map((e: [EffectResponseData, string]) => e[0]);
                callStackMap.set(JSON.stringify(baseEffect), callStack);
            }

            const auditedEffects = Array.from(effectsMap)
                .filter(([_, value]) => value !== 'Skipped')
                .map(([key, _]) => key);
            
            locationsProvider.clear();
            locationsProvider.addAuditedEffects(auditedEffects);
            locationsProvider.setLocations([...effectsMap.keys()], callStackMap);
            annotations.setPreviousAnnotations(locationsProvider.getGroupedEffects(), effectsMap);

            const editor = vscode.window.activeTextEditor;
            if(editor) {
                highlightEffectLocations(editor, locationsProvider.getGroupedEffects());
            }
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.set_annotation', async (effect: EffectResponseData, ann: string) => {
            const chain_audit_mode = context.globalState.get('chainAudit');
            annotations.trackUserAnnotations(effect, ann);

            if (ann === 'Skipped') {
                locationsProvider.unmarkAuditedEffect(effect);
            }
            else {
                locationsProvider.addAuditedEffects([effect])
            }

            const eff = {
                ...effect,
                location: {
                    uri: effect.location.uri.toString(),
                    range: effect.location.range
                }
            };

            // Notify server about the received safety annotation from the user
            client.sendNotification('cargo-scan.set_annotation', {
                safety_annotation: ann,
                effect: eff,
                chain_audit_mode
            }); 
            
            // If we're annotating effects in a chain audit,
            // reload chain to update the previewed effects
            if (chain_audit_mode) {
                vscode.commands.executeCommand('cargo-scan.audit_chain');
            }
            else {
                // If we're updating an effect annotation, a caller-checked
                // hierarchy might also change. Reload the audit to update it
                vscode.commands.executeCommand('cargo-scan.audit');
            }
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.filterEffects', () => {
            const effectTypes = [
                '[Sink Call]',
                '[PtrDeref]',
                '[FFI Call]',
                '[UnsafeCall]',
                '[UnionField]',
                '[StaticMutVar]',
                '[StaticExtVar]',
                '[FnPtrCreation]',
                '[ClosureCreation]',
                '[FFI Declaration]'
            ];

            vscode.window.showQuickPick(
                effectTypes, 
                { canPickMany: true, placeHolder: 'Select filters' }
            )
            .then(input => {
                if (input !== undefined && input.length > 0) {
                    const filters = input.length === effectTypes.length
                        ? ["[All]"] 
                        : [ ...input ];
                    locationsProvider.filterEffectsByType(filters);
                }
            });     
        })
    );

    context.subscriptions.push(
        vscode.window.onDidChangeActiveTextEditor(editor => {
            const auditMode =  context.globalState.get('annotateEffects');
            if (editor && auditMode) {
                highlightEffectLocations(editor, locationsProvider.getGroupedEffects());
            }
        })
    );

    context.subscriptions.push(
        vscode.workspace.onDidChangeTextDocument(event => {
            const auditMode =  context.globalState.get('annotateEffects');
            const editor = vscode.window.visibleTextEditors.find(e => e.document === event.document);
            if (editor && auditMode) {
                highlightEffectLocations(editor, locationsProvider.getGroupedEffects());
            }
        })
    );

    vscode.commands.registerCommand('cargo-scan.viewCallers', (item: LocationItem) => {
        locationsProvider.filterByCallers(item);
    });

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.refreshEffects', () => {
            locationsProvider.restore();     
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.view_annotations', async (effects: EffectResponseData[]) => { 
            const callees: string[] = effects.map(effect => effect.callee);
            const input = await vscode.window.showQuickPick( callees, { placeHolder: 'Select effect to audit' });
            const selected = effects.find(e => e.callee === input);
            annotations.effectToAudit(selected); 
        })
    );

}