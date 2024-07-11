import * as vscode from 'vscode';
import { EffectResponseData, EffectsResponse } from './file_tree_view';
import { annotations, client, locationsProvider } from './extension';
import { AuditResponse } from './audit_annotations';

export function registerCommands(context: vscode.ExtensionContext) {
    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.scan', async () => {
            const response = await client.sendRequest<EffectsResponse>('cargo-scan.scan');
            context.globalState.update('annotateEffects', false);
            
            locationsProvider.setLocations(response.effects);
            annotations.clearAnnotations();
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.audit', async () => {
            const response = await client.sendRequest<AuditResponse>('cargo-scan.audit');
            context.globalState.update('annotateEffects', true);

            const effectsMap = new Map<EffectResponseData, string>(response.effects);                  
            locationsProvider.setLocations([...effectsMap.keys()]);                       
            annotations.setPreviousAnnotations(locationsProvider.getGroupedEffects(), effectsMap);
        })
    );  
    
    context.subscriptions.push(annotations.onDidChangeCodeLenses(() => {
        vscode.commands.executeCommand('editor.action.codeLens.refresh');
    }));

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.safeAnnotation', async (effect: EffectResponseData) => {
            annotations.trackUserAnnotations(effect, 'Safe');
            
            // Notify server about the received safety annotation from the user
            client.sendNotification('cargo-scan.set_annotation', { safety_annotation: 'Safe', effect });                           
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.unsafeAnnotation', async (effect: EffectResponseData) => {
            annotations.trackUserAnnotations(effect, 'Unsafe');  

            // Notify server about the received safety annotation from the user
            client.sendNotification('cargo-scan.set_annotation', { safety_annotation: 'Unsafe', effect });                           
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.ccAnnotation', async (effect: EffectResponseData) => {
            annotations.trackUserAnnotations(effect, 'Caller-checked');

            // Notify server about the received safety annotation from the user
            client.sendNotification('cargo-scan.set_annotation', { safety_annotation: 'Caller-Checked', effect });                           
        })
    );
}
