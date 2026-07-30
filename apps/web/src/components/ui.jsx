import React from 'react';
import * as DialogPrimitive from '@radix-ui/react-dialog';
import * as LabelPrimitive from '@radix-ui/react-label';
import * as SelectPrimitive from '@radix-ui/react-select';
import * as TabsPrimitive from '@radix-ui/react-tabs';
import { Check, ChevronDown, X } from 'lucide-react';
import { cn } from '../lib/utils';

export function Button({ className, variant = 'default', size = 'default', ...props }) {
  return <button className={cn('button', `button-${variant}`, `button-${size}`, className)} {...props} />;
}

export function Card({ className, ...props }) {
  return <section className={cn('card', className)} {...props} />;
}

export function CardHeader({ className, ...props }) {
  return <div className={cn('card-header', className)} {...props} />;
}

export function CardTitle({ className, ...props }) {
  return <h2 className={cn('card-title', className)} {...props} />;
}

export function CardContent({ className, ...props }) {
  return <div className={cn('card-content', className)} {...props} />;
}

export function Input({ className, ...props }) {
  return <input className={cn('input', className)} {...props} />;
}

export function Textarea({ className, ...props }) {
  return <textarea className={cn('textarea', className)} {...props} />;
}

export const Label = React.forwardRef(function Label({ className, ...props }, ref) {
  return <LabelPrimitive.Root ref={ref} className={cn('label', className)} {...props} />;
});

export function Field({ label, hint, children }) {
  return (
    <div className="field">
      {label && <Label>{label}</Label>}
      {children}
      {hint && <span className="field-hint">{hint}</span>}
    </div>
  );
}

export function Badge({ className, variant = 'default', children, ...props }) {
  return (
    <span className={cn('badge', `badge-${variant}`, className)} {...props}>
      {children}
    </span>
  );
}

export function Avatar({ name, className, size = 'md' }) {
  const text = initials(name);
  return (
    <span className={cn('avatar', `avatar-${size}`, className)} aria-hidden="true" title={name || ''}>
      {text}
    </span>
  );
}

function initials(value) {
  const clean = String(value || '').trim();
  if (!clean) return '?';
  const parts = clean.split(/[\s@._-]+/).filter(Boolean);
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogClose = DialogPrimitive.Close;

export function DialogContent({ className, children, ...props }) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="dialog-overlay" />
      <DialogPrimitive.Content className={cn('dialog-content', className)} {...props}>
        {children}
        <DialogPrimitive.Close className="dialog-close" aria-label="Close">
          <X size={16} />
        </DialogPrimitive.Close>
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}

export function DialogHeader({ className, ...props }) {
  return <div className={cn('dialog-header', className)} {...props} />;
}

export const DialogTitle = DialogPrimitive.Title;
export const DialogDescription = DialogPrimitive.Description;

// Sheet = right-side slide-over, built on the Dialog primitive.
export const Sheet = DialogPrimitive.Root;
export const SheetTrigger = DialogPrimitive.Trigger;
export const SheetClose = DialogPrimitive.Close;

export function SheetContent({ className, children, ...props }) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="sheet-overlay" />
      <DialogPrimitive.Content className={cn('sheet-content', className)} {...props}>
        {children}
        <DialogPrimitive.Close className="sheet-close" aria-label="Close">
          <X size={18} />
        </DialogPrimitive.Close>
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}

export const Tabs = TabsPrimitive.Root;

export const TabsList = React.forwardRef(function TabsList({ className, ...props }, ref) {
  return <TabsPrimitive.List ref={ref} className={cn('tabs-list', className)} {...props} />;
});

export const TabsTrigger = React.forwardRef(function TabsTrigger({ className, ...props }, ref) {
  return <TabsPrimitive.Trigger ref={ref} className={cn('tabs-trigger', className)} {...props} />;
});

export const TabsContent = React.forwardRef(function TabsContent({ className, ...props }, ref) {
  return <TabsPrimitive.Content ref={ref} className={cn('tabs-content', className)} {...props} />;
});

export function Select({ value, onValueChange, children }) {
  return (
    <SelectPrimitive.Root value={value} onValueChange={onValueChange}>
      {children}
    </SelectPrimitive.Root>
  );
}

export function SelectTrigger({ className, children, ...props }) {
  return (
    <SelectPrimitive.Trigger className={cn('select-trigger', className)} {...props}>
      <SelectPrimitive.Value>{children}</SelectPrimitive.Value>
      <SelectPrimitive.Icon>
        <ChevronDown size={15} />
      </SelectPrimitive.Icon>
    </SelectPrimitive.Trigger>
  );
}

export function SelectContent({ className, children, ...props }) {
  return (
    <SelectPrimitive.Portal>
      <SelectPrimitive.Content className={cn('select-content', className)} position="popper" sideOffset={6} {...props}>
        <SelectPrimitive.Viewport>{children}</SelectPrimitive.Viewport>
      </SelectPrimitive.Content>
    </SelectPrimitive.Portal>
  );
}

export function SelectItem({ className, children, ...props }) {
  return (
    <SelectPrimitive.Item className={cn('select-item', className)} {...props}>
      <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
      <SelectPrimitive.ItemIndicator className="select-indicator">
        <Check size={14} />
      </SelectPrimitive.ItemIndicator>
    </SelectPrimitive.Item>
  );
}

export function Table({ className, ...props }) {
  return <div className={cn('table', className)} role="table" {...props} />;
}

export function TableHeader({ className, ...props }) {
  return <div className={cn('table-header', className)} role="rowgroup" {...props} />;
}

export function TableBody({ className, ...props }) {
  return <div className={cn('table-body', className)} role="rowgroup" {...props} />;
}

export function TableRow({ className, ...props }) {
  return <div className={cn('table-row', className)} role="row" {...props} />;
}

export function TableHead({ className, ...props }) {
  return <div className={cn('table-head', className)} role="columnheader" {...props} />;
}

export function TableCell({ className, ...props }) {
  return <div className={cn('table-cell', className)} role="cell" {...props} />;
}

export function Alert({ className, ...props }) {
  return <div className={cn('alert', className)} role="alert" {...props} />;
}
