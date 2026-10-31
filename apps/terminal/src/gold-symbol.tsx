// The gold-bars mark is drawn as a small vector so it stays sharp at all sizes.
export function GoldSymbol({className}:{className:string}){
 return <span className={className} aria-hidden="true"><svg viewBox="0 0 24 24" fill="currentColor" width="72%" height="72%"><path d="M8.2 3.5h7.6l2 4.1-2.1 2H8.3l-2.1-2 2-4.1Zm.7 2.2-.8 1.6h7.8l-.8-1.6H8.9ZM3.1 12h7.5l1.9 4-2 2H3.3l-2-2 1.8-4Zm.8 2.2-.7 1.5h7l-.7-1.5H3.9ZM13.5 12h7.4l1.8 4-2 2h-7.3l-1.9-2 2-4Zm.8 2.2-.8 1.5h7.1l-.7-1.5h-5.6Z"/></svg></span>;
}
