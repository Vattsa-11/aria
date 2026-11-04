declare module "des.js" {
  const des: { DES: { create(options: { type: "encrypt" | "decrypt"; key: number[]; padding?: boolean }): { update(data: number[]): number[]; final(): number[] } } };
  export default des;
}
