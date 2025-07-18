use swc_core::ecma::ast::*;
use swc_core::ecma::visit::VisitMut;

pub struct StripTypeOnlyImports;

impl VisitMut for StripTypeOnlyImports {
    fn visit_mut_module_items(&mut self, items: &mut Vec<ModuleItem>) {
        items.retain_mut(|item| {
            if let ModuleItem::ModuleDecl(ModuleDecl::Import(import_decl)) = item {
                // 保留非 type-only 的 specifier
                import_decl.specifiers.retain(|s| match s {
                    ImportSpecifier::Named(named) => !named.is_type_only,
                    _ => true,
                });

                // 如果没有任何 specifier 了，移除整个 import
                !import_decl.specifiers.is_empty()
            } else {
                true
            }
        });
    }
}
