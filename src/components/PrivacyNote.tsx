interface PrivacyNoteProps {
  items: string[];
}

export function PrivacyNote({ items }: PrivacyNoteProps) {
  return (
    <ul className="privacy-note">
      {items.map((item) => (
        <li key={item}>{item}</li>
      ))}
    </ul>
  );
}
