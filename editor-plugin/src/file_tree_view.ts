import * as vscode from 'vscode';
import * as path from 'path';
import { DecorationProvider } from './decorations';

export interface EffectResponseData {
    caller: string;
    callee: string;
    effect_type: string;
    location: vscode.Location;
    crate_name: string;
}

export interface EffectsResponse {
    effects: EffectResponseData[];
}

// Class to present the locations of the effects as a TreeView in VSCode's sidebar
export class LocationsProvider implements vscode.TreeDataProvider<vscode.TreeItem> {
    private _onDidChangeTreeData: vscode.EventEmitter<vscode.TreeItem | undefined> =
        new vscode.EventEmitter<vscode.TreeItem | undefined>();
    readonly onDidChangeTreeData: vscode.Event<vscode.TreeItem | undefined> =
        this._onDidChangeTreeData.event;

    private currentFilters: string[] = ["[All]"];
    private audited: Set<EffectResponseData> = new Set();
    private callStack?: Map<string, EffectResponseData[]>;
    private groupedEffects:  { [file: string]: EffectResponseData[] } = {};
    private filteredEffects: { [file: string]: EffectResponseData[] } = {};

    setLocations(
        effects: EffectResponseData[],
        callStack?: Map<string, EffectResponseData[]>
    ) {
        if (callStack !== undefined) {
            this.callStack = callStack;
        }
        this.groupByFile(effects);
        this.sortGroupedEffects();
        this.filterEffectsByType(this.currentFilters);
        this.refresh();    
    }

    updateEffectCallStack(baseEffect: EffectResponseData, callers: EffectResponseData[]) {
        const baseEffectStr = JSON.stringify(baseEffect);
        this.callStack = this.callStack ?? new Map<string, EffectResponseData[]>();

        if (this.callStack.has(baseEffectStr)) {
            const currentStack = this.callStack.get(baseEffectStr);
            if (currentStack) {
                this.callStack.set(baseEffectStr, [...currentStack, ...callers]);
            }
        } else {
            this.callStack.set(baseEffectStr, [baseEffect, ...callers]);
        }

        this.refresh();
    }

    addAuditedEffects(effects: EffectResponseData[]) {
        effects.forEach(e => this.audited.add(e));
        this.refresh();
    }

    unmarkAuditedEffect(effect: EffectResponseData) {
        this.audited.delete(effect);
        this.refresh();
    }

    getTreeItem(element: LocationItem): vscode.TreeItem {
        return element;
    }

    getChildren(element?: vscode.TreeItem): Thenable<vscode.TreeItem[]> {
        if (!element) {
            const crates = this.getCrateItems();
            if (crates.length <= 1) {
                return Promise.resolve(crates);
            }

            const root = new DirectoryItem(vscode.Uri.file("root"), "Root Crate");
            const deps = new DirectoryItem(vscode.Uri.file("dependencies"), "Dependencies");
            const wsName = vscode.workspace.workspaceFolders?.map((folder) => folder.name)[0];
            crates.forEach(crate => crate.label === wsName ? root.addChild(crate) : deps.addChild(crate));

            root.updateDecorations(false);
            deps.updateDecorations(true);

            return Promise.resolve([root, deps]);
        } else if (element instanceof DirectoryItem) {
            return Promise.resolve(element.getChildren());
        } else if (element instanceof FileItem) {
            return Promise.resolve(element.getChildren());
        }
        return Promise.resolve([]);
    }

    // Group effects by their containing file
    private groupByFile(effects: EffectResponseData[]) {
        for (const effect of effects) {
            const uri =  effect.location.uri;
            const file = uri.fsPath;

            if (!this.groupedEffects[file]) {
                this.groupedEffects[file] = [];
            }
            
            if (!this.groupedEffects[file].some(e => e.location.range.isEqual(effect.location.range) 
                && e.callee == effect.callee)) {
                this.groupedEffects[file].push(effect);
            }
        }
    }

    private getCrateItems(): vscode.TreeItem[] {
        const crates: vscode.TreeItem[] = [];
    
        // Loop through the effects and build directory hierarchy
        for (const file in this.filteredEffects) {
            const effects = this.filteredEffects[file];
            if (effects.length === 0) {
                continue;
            }
            const crateName = effects[0].crate_name; 
            const relativePath = this.getRelativeFilePath(file, crateName);
            this.buildDirectories(relativePath, effects, crates);
        }

        crates.forEach(item => {
            if (item instanceof DirectoryItem) {
                item.updateDescription();
            }
        });

        // Sort alphabetically by crate name
        return crates.sort((a, b) => {
            const aLabel = typeof a.label === 'string' ? a.label : a.label?.label ?? '';
            const bLabel = typeof b.label === 'string' ? b.label : b.label?.label ?? '';
            return aLabel.localeCompare(bLabel);
        });
    }

    // Get the file path relative to the crate root
    private getRelativeFilePath(filePath: string, crateName: string): string {
        const segments = filePath.split(path.sep);    
        const name = crateName.replace(/_/g, '-');
        const idx = segments.findIndex((segment) => {
            return segment.replace(/_/g, '-').startsWith(name);
        });
    
        return idx !== -1 ? path.join(...segments.slice(idx)) : filePath;
    }
    

    private buildDirectories(
        filePath: string,
        effects: EffectResponseData[],
        parentItems: vscode.TreeItem[]
    ) {          
        const segments = filePath.split(path.sep);
        segments.forEach((segment, index) => {
            if (index === segments.length - 1) {
                // The last segment is the filename
                const fileItem = new FileItem(vscode.Uri.file(filePath), effects, this.audited, this.callStack);
                parentItems.push(fileItem);
            } else {
                let dirItem = parentItems.find(
                    (item) => item instanceof DirectoryItem && item.label === segment
                ) as DirectoryItem;
    
                if (!dirItem) {
                    dirItem = new DirectoryItem(vscode.Uri.file(filePath), segment);
                    parentItems.push(dirItem);
                }
    
                parentItems = dirItem.getChildren();
            }
        });
    }
    
    getGroupedEffects(): { [file: string]: EffectResponseData[] } {
        return this.groupedEffects;
    }

    register(context: vscode.ExtensionContext) {
        vscode.window.registerFileDecorationProvider(DecorationProvider);
        const tree = vscode.window.createTreeView('effectsView', { treeDataProvider: this, showCollapseAll: true });
        tree.onDidChangeCheckboxState(async (event) => {
            // Get the first item that triggered the event,
            // which is an audited effect that the user wants
            // to reset its safety annotation.
            const item = event.items[0];
            const effect = (item[0] as LocationItem).data;

            if (item[1] === vscode.TreeItemCheckboxState.Unchecked) {
                // Confirm that the user wants to proceed with the resetting
                const result = await vscode.window.showWarningMessage(
                    'Are you sure you want to reset this annotation?', 
                    { modal: true },
                    'Yes',
                    'No'
                );

                if ( result === 'Yes' ) {
                    vscode.commands.executeCommand('cargo-scan.set_annotation', effect, 'Skipped');
                }
                else {
                    item[0].checkboxState = vscode.TreeItemCheckboxState.Checked;
                    this.refresh();
                }
            }
        });

        context.subscriptions.push(tree);
    }

    clear() {
        this.audited.clear();
        this.groupedEffects = {};
        this.filteredEffects = {};
        this.callStack = undefined;
        this.currentFilters = ["[All]"];
        this.refresh();
    }

    refresh() {
        this._onDidChangeTreeData.fire(undefined);
    }

    private sortGroupedEffects() {
        for (const file in this.groupedEffects) {
            this.groupedEffects[file].sort((a, b) => 
                a.location.range.start.compareTo(b.location.range.start));
        }
    }

    // Filter effects that match a given type
    filterEffectsByType(filters: string[]) {
        if(!filters)
            return;
    
        this.currentFilters = [ ...filters ];
        if( filters.includes("[All]") ) {
            this.filteredEffects = { ...this.groupedEffects };
            this.refresh();
            return;
        }

        for (const [file, effects] of Object.entries(this.groupedEffects)) {
            this.filteredEffects[file] = [
                ...effects.filter(e => {
                    const ty = e.effect_type.startsWith('[') ? e.effect_type : "[Sink Call]";
                    return filters.includes(ty)
                })
            ];
        }

        this.refresh();
    }

    filterByCallers(item: LocationItem) {
        if (item.contextValue == 'isCC') {
            const callers = this.callStack?.get(JSON.stringify(item.data));
            if (callers && callers.length > 1) {
                this.filteredEffects = {};
                
                for (const caller of callers) {
                    const file = caller.location.uri.fsPath;
                    if (!this.filteredEffects[file]) {
                        this.filteredEffects[file] = [];
                    }
                    this.filteredEffects[file].push(caller);
                }
            }
            this.refresh();
        }  
    }
}

class DirectoryItem extends vscode.TreeItem {
    private children: vscode.TreeItem[] = [];
    private total: number;
    private unaudited: number;
    constructor(
        public readonly resourceUri: vscode.Uri,
        public readonly label: string
    ) {
        super(label, vscode.TreeItemCollapsibleState.Collapsed);
        this.total = 0;
        this.unaudited = 0;
    }

    getChildren(): vscode.TreeItem[] {
        return this.children;
    }

    totalEffects(): number {
        return this.total;
    }

    totalUnaudited(): number {
        return this.unaudited;
    }

    updateDescription() {
        for (const child of this.children) {
            if (child instanceof DirectoryItem) {
                child.updateDescription();
                this.total += child.totalEffects();
                this.unaudited += child.totalUnaudited();
            }
            else if (child instanceof FileItem) {
                this.total += child.totalEffects();
                this.unaudited += child.totalUnaudited();
            }
        }

        this.description = this.total > 0
            ? `[ ${this.total - this.unaudited} / ${this.total} ]` 
            : undefined;
    }

    updateDecorations(deps: boolean) {
        const symbol = deps ? 'type-hierarchy' : 'symbol-folder';
        this.iconPath = new vscode.ThemeIcon(symbol);
        DecorationProvider.decorateChainRoots(this.resourceUri);
    }

    addChild(item: vscode.TreeItem) {
        this.children.push(item);
    }
}

class FileItem extends vscode.TreeItem {
    private total: number = 0;
    private unaudited: number = 0;
    constructor(
        public readonly resourceUri: vscode.Uri,
        public readonly effects: EffectResponseData[],
        private readonly audited: Set<EffectResponseData>,
        private readonly callStack?: Map<string, EffectResponseData[]>
    ) {
        const label = `${path.basename(resourceUri.fsPath)}`;
        super(label, vscode.TreeItemCollapsibleState.Collapsed);
        this.updateDecorations();
    }

    private updateDecorations() {  
        this.total = this.effects.length;
        this.unaudited = this.effects.reduce((count, item) => {
            return !this.audited.has(item) ? count += 1 : count;
        }, 0);

        this.description = `[ ${this.total - this.unaudited} / ${this.total} ]`;        
        DecorationProvider.updateDecorations(this.resourceUri, this.unaudited);
    }

    totalEffects(): number {
        return this.total;
    }

    totalUnaudited(): number {
        return this.unaudited;
    }

    getChildren(): vscode.TreeItem[] {
        // Return the effects' locations within the file
        return this.effects.map((effect) => {
            const state = this.audited.has(effect) 
                ? vscode.TreeItemCheckboxState.Checked : undefined;
            
            const isCCBaseEffect = (this.callStack?.get(JSON.stringify(effect))?.length ?? 0) > 1;
            return new LocationItem(effect, state, isCCBaseEffect);
        });
    }
}

export class LocationItem extends vscode.TreeItem {
    constructor(
        public readonly data: EffectResponseData, 
        public readonly state: vscode.TreeItemCheckboxState | undefined,
        private readonly isCCBaseEffect: boolean
    ) {
        let start = data.location.range.start;
        super(
            `${data.effect_type}`,
            vscode.TreeItemCollapsibleState.None
        );

        this.checkboxState = state;
        this.tooltip = data.callee;
        this.command = {
            command: 'vscode.open',
            title: 'Open Location',
            arguments: [data.location.uri, { selection: data.location.range }]
        };
        this.description = `${start.line + 1}:${start.character + 1}`;
        this.contextValue = this.isCCBaseEffect ? 'isCC' : undefined;
    }
}
